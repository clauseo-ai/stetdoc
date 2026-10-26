//! The part of a sweep that runs over any list: the workers, the watchdog, the deadline and what they say. `laws::sweep` runs it
//! over the corpus, and its tests over a list with a fake law.

use super::{ENV_DEADLINE, ENV_LIMIT, SLOWEST, Settings, Swept};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// What a sweep says, for [`sweep`]'s report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Said {
    /// A failure, the moment it is found.
    Failure,
    /// A document that a survey's law does not hold for ([`Settings::misses`]).
    Miss,
    /// How far the sweep got.
    Progress,
    /// A document that is still running past the watchdog.
    Watch,
    /// The deadline passed.
    Deadline,
    /// The end: counts, and the slowest documents.
    Summary,
}

/// What one document came to.
#[derive(Debug)]
pub enum Outcome {
    /// The law ran, with its result.
    Checked(Result<(), String>),
    /// `open` refused it as malformed input.
    Skipped,
    /// It failed before the law ran, with a sentence that counts as a failure.
    Failed(String),
}

/// What one worker is on: the document, when it started, and whether the watchdog and the limit have named it.
struct Slot {
    id: String,
    started: Instant,
    watched: bool,
    over: bool,
}

/// Runs `work` over `items` with `settings`, and says each failure, the progress, the watchdog's finds, the deadline and the
/// summary through `say`, the moment they happen. `name` names the sweep in each line; `id` gives an item's document id.
pub fn sweep<T: Sync>(
    name: &str,
    items: &[T],
    id: impl Fn(&T) -> &str + Sync,
    settings: &Settings,
    say: &(dyn Fn(Said, &str) + Sync),
    work: impl Fn(&T) -> Outcome + Sync,
) -> Swept {
    let begun = Instant::now();
    let workers = settings.workers.max(1).min(items.len().max(1));
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let stopped = AtomicBool::new(false);
    let slots: Vec<Mutex<Option<Slot>>> = (0..workers).map(|_| Mutex::new(None)).collect();
    let found = Mutex::new(Found::default());
    let finished = (Mutex::new(false), Condvar::new());
    let fail = |line: String| {
        failed.fetch_add(1, Ordering::Relaxed);
        say(Said::Failure, &format!("{name}: FAILED {line}"));
        if let Ok(mut f) = found.lock() {
            f.failures.push(line);
        }
    };
    let past_deadline = || settings.deadline.is_some_and(|d| SystemTime::now() >= d);
    std::thread::scope(|scope| {
        let handles: Vec<_> = slots
            .iter()
            .map(|slot| {
                let (next, done, failed, stopped, found, fail, id, work) =
                    (&next, &done, &failed, &stopped, &found, &fail, &id, &work);
                scope.spawn(move || {
                    loop {
                        if past_deadline() {
                            if !stopped.swap(true, Ordering::Relaxed) {
                                say(
                                    Said::Deadline,
                                    &format!(
                                        "{name}: the deadline ({ENV_DEADLINE}) passed after {}, so no worker takes a new document",
                                        clock(begun.elapsed())
                                    ),
                                );
                            }
                            break;
                        }
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(i) else { break };
                        let doc = id(item).to_string();
                        if let Ok(mut s) = slot.lock() {
                            *s = Some(Slot {
                                id: doc.clone(),
                                started: Instant::now(),
                                watched: false,
                                over: false,
                            });
                        }
                        let started = Instant::now();
                        let outcome = work(item);
                        let took = started.elapsed();
                        let named_over = slot
                            .lock()
                            .ok()
                            .and_then(|mut s| s.take())
                            .is_some_and(|s| s.over);
                        match outcome {
                            Outcome::Checked(result) => {
                                if let Ok(mut f) = found.lock() {
                                    f.checked.push(doc.clone());
                                }
                                match result {
                                    Ok(()) => {}
                                    Err(why) if settings.misses => {
                                        say(Said::Miss, &format!("{name}: missed {doc}: {why}"));
                                        if let Ok(mut f) = found.lock() {
                                            f.failures.push(format!("{doc}: {why}"));
                                        }
                                    }
                                    Err(why) => fail(format!("{doc}: {why}")),
                                }
                            }
                            Outcome::Skipped => {
                                if let Ok(mut f) = found.lock() {
                                    f.skipped.push(doc.clone());
                                }
                            }
                            Outcome::Failed(why) => fail(format!("{doc}: {why}")),
                        }
                        if took > settings.limit {
                            let line = over_limit(&doc, took, settings.limit, false);
                            // The watchdog said it while it ran, or it is said now; it counts once, with its whole time.
                            if !named_over {
                                failed.fetch_add(1, Ordering::Relaxed);
                                say(Said::Failure, &format!("{name}: FAILED {line}"));
                            }
                            if let Ok(mut f) = found.lock() {
                                f.over_limit.push(line);
                            }
                        }
                        if let Ok(mut f) = found.lock() {
                            f.times.push((took, doc));
                        }
                        done.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();
        let watchdog = scope.spawn(|| {
            let mut last = Instant::now();
            let (lock, wake) = &finished;
            let Ok(mut over) = lock.lock() else { return };
            while !*over {
                over = match wake.wait_timeout(over, Duration::from_millis(250)) {
                    Ok((g, _)) => g,
                    Err(_) => return,
                };
                for slot in &slots {
                    let Ok(mut s) = slot.lock() else { continue };
                    let Some(s) = s.as_mut() else { continue };
                    let ran = s.started.elapsed();
                    if !s.watched && ran > settings.watch {
                        s.watched = true;
                        say(
                            Said::Watch,
                            &format!(
                                "{name}: {} is still running after {}, over the watchdog's {} s",
                                s.id,
                                clock(ran),
                                settings.watch.as_secs()
                            ),
                        );
                    }
                    if !s.over && ran > settings.limit {
                        s.over = true;
                        failed.fetch_add(1, Ordering::Relaxed);
                        say(
                            Said::Failure,
                            &format!(
                                "{name}: FAILED {}",
                                over_limit(&s.id, ran, settings.limit, true)
                            ),
                        );
                    }
                }
                if last.elapsed() >= settings.progress {
                    last = Instant::now();
                    let running = slots
                        .iter()
                        .filter(|s| s.lock().is_ok_and(|s| s.is_some()))
                        .count();
                    say(
                        Said::Progress,
                        &format!(
                            "{name}: {} of {} documents done in {}, {} failures so far, {running} running",
                            done.load(Ordering::Relaxed),
                            items.len(),
                            clock(begun.elapsed()),
                            failed.load(Ordering::Relaxed)
                        ),
                    );
                }
            }
        });
        for h in handles {
            let _ = h.join();
        }
        if let Ok(mut over) = finished.0.lock() {
            *over = true;
        }
        finished.1.notify_all();
        let _ = watchdog.join();
    });
    let found = found.into_inner().unwrap_or_default();
    let swept = found.into_swept(items.len(), done.load(Ordering::Relaxed));
    say(
        Said::Summary,
        &summary(name, &swept, begun.elapsed(), settings.misses),
    );
    swept
}

/// What the workers gather.
#[derive(Default)]
struct Found {
    checked: Vec<String>,
    skipped: Vec<String>,
    failures: Vec<String>,
    over_limit: Vec<String>,
    times: Vec<(Duration, String)>,
}

impl Found {
    fn into_swept(self, chosen: usize, done: usize) -> Swept {
        let Found {
            mut checked,
            mut skipped,
            mut failures,
            mut over_limit,
            mut times,
        } = self;
        over_limit.sort();
        checked.sort();
        skipped.sort();
        failures.sort();
        times.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        times.truncate(SLOWEST);
        Swept {
            checked: checked.len(),
            checked_ids: checked,
            skipped,
            failures,
            over_limit,
            chosen,
            not_reached: chosen.saturating_sub(done),
            slowest: times,
            ..Swept::default()
        }
    }
}

fn over_limit(id: &str, took: Duration, limit: Duration, running: bool) -> String {
    format!(
        "{id}: {} {}, over the limit of {} s for one document (a performance bug; {ENV_LIMIT} sets the limit)",
        if running {
            "still running after"
        } else {
            "took"
        },
        clock(took),
        limit.as_secs()
    )
}

/// The lines that end a sweep: what it did, and its slowest documents.
fn summary(name: &str, swept: &Swept, took: Duration, settings_misses: bool) -> String {
    let mut out = format!(
        "{name}: {} documents checked and {} skipped of {} in {}, {} {}, {} over the time limit, {} not reached",
        swept.checked,
        swept.skipped.len(),
        swept.chosen,
        clock(took),
        swept.failures.len(),
        if settings_misses {
            "misses"
        } else {
            "failures"
        },
        swept.over_limit.len(),
        swept.not_reached
    );
    if !swept.slowest.is_empty() {
        let _ = write!(out, "\n{name}: the slowest {}:", swept.slowest.len());
        for (t, id) in &swept.slowest {
            let _ = write!(out, "\n  {:>8.1} s  {id}", t.as_secs_f64());
        }
    }
    out
}

/// A duration as `1h 02m 03s`, `2m 03s` or `3.4 s`.
pub fn clock(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..60 => format!("{:.1} s", d.as_secs_f64()),
        60..3600 => format!("{}m {:02}s", s / 60, s % 60),
        _ => format!("{}h {:02}m {:02}s", s / 3600, s % 3600 / 60, s % 60),
    }
}
