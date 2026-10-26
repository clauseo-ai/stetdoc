//! Runs a law over every document of the corpus set on every core, and says what it found, as it finds it.
//!
//! The laws of `laws::*` each take the bytes of a document and return `Err` with a sentence. This runs one over the documents
//! of the set that must open (`expect` ok), and lists the failures sorted by document, so that one run names every document that
//! breaks a law. A document that `Document::open` refuses as malformed input is not a Word document (the container and XML layers
//! accept more than `State::open` does) and is counted as skipped, with its id, and a set in which more than one document in a
//! hundred is skipped is a failure of its own, so that a regression in `open` cannot hide as a skip.
//!
//! The nightly `laws` workflow runs the full corpora in shards on machines that a timeout can stop, so a sweep is built to
//! leave a useful log however it ends:
//!
//! - **Shards.** `STETDOC_SHARD=i/n` keeps the documents whose SHA-256 (from `INDEX.tsv`) is `i` modulo `n` ([`Shard`]), chosen
//!   before a document is read, so a shard reads, opens and counts only its own documents.
//! - **Streaming.** Each failure is printed the moment it is found, and progress every few minutes (`STETDOC_PROGRESS_SECS`, 180
//!   by default): documents done of the total, the time, the failures so far.
//! - **Time.** Each document's time is kept, and the slowest 20 are printed at the end. A watchdog names a document still running
//!   after `STETDOC_WATCH_SECS` (120 by default), and a document over `STETDOC_LIMIT_SECS` (600 by default) is a failure of the
//!   sweep: a performance bug. Rust cannot stop a running thread, so the watchdog names a runaway in the log before a timeout
//!   kills the job.
//! - **Deadline.** After `STETDOC_DEADLINE` (Unix seconds) no worker takes a new document; the sweep reports what it found and
//!   how many documents it did not reach, and fails when it left any.
//!
//! [`Settings`] holds these, and [`sweep`] is the part that runs over any list, which the tests drive with a fake law.

use crate::corpus::{Corpus, Entry};
use std::io::Write as _;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use stetdoc_core::{Document, ErrorCode, OpenOptions};

mod run;
#[cfg(test)]
mod tests;

pub use run::{Outcome, Said, clock, sweep};

/// Runs `check` on every item on as many threads as the machine has, and returns the failures, sorted. A document is checked on
/// one thread, and a `Document` is `Send`, so the checks do not share anything.
pub fn in_parallel<T: Sync>(
    items: &[T],
    check: impl Fn(&T) -> Option<String> + Sync,
) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(items.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if let Some(why) = check(item)
                        && let Ok(mut list) = failures.lock()
                    {
                        list.push(why);
                    }
                }
            });
        }
    });
    let mut out = failures.into_inner().unwrap_or_default();
    out.sort();
    out
}

/// A failure of a law that is known, and has an owner. A gap is the id of a document, a piece of the failure's sentence, and why it
/// happens and whose it is. The sweep takes it out of the failures, and **a gap that no longer occurs on a document that was
/// checked is a failure of its own**, so that a fix cannot go unnoticed and the list cannot rot.
#[derive(Clone, Copy, Debug)]
pub struct Gap {
    /// The id of the document, `<source>:<path in the source>`.
    pub id: &'static str,
    /// A piece of the sentence that the failure ends in.
    pub says: &'static str,
    /// Why it happens, and who fixes it.
    pub why: &'static str,
}

/// What a sweep of the corpus found.
#[derive(Debug, Default)]
pub struct Swept {
    /// Documents the law ran on.
    pub checked: usize,
    /// The ids of those documents, sorted.
    pub checked_ids: Vec<String>,
    /// Documents that `open` refused as malformed input, by id.
    pub skipped: Vec<String>,
    /// The sentences of the law's failures, with the id of the document, sorted.
    pub failures: Vec<String>,
    /// The documents that ran longer than the limit ([`Settings::limit`]), each a sentence with its id and time, sorted.
    pub over_limit: Vec<String>,
    /// Whether `STETDOC_ONLY` chose the documents, so that the sweep is a rerun of a few and not a run of the set.
    pub only: bool,
    /// The shard the sweep ran, when `STETDOC_SHARD` named one.
    pub shard: Option<Shard>,
    /// The documents of the set that must open, before the shard and `STETDOC_ONLY` chose among them.
    pub in_set: usize,
    /// The documents the sweep was to run: those of its shard, or those `STETDOC_ONLY` named.
    pub chosen: usize,
    /// Documents of the sweep that no worker started before the deadline.
    pub not_reached: usize,
    /// The slowest documents, slowest first, at most [`SLOWEST`].
    pub slowest: Vec<(Duration, String)>,
}

/// How many of the slowest documents a sweep keeps and prints.
pub const SLOWEST: usize = 20;

impl Swept {
    /// The sweep without the known gaps. A gap whose document was checked and whose failure did not occur is an error, naming it.
    pub fn minus(mut self, gaps: &[Gap]) -> Result<Swept, String> {
        let mut stale = Vec::new();
        for gap in gaps {
            if self
                .checked_ids
                .binary_search_by(|id| id.as_str().cmp(gap.id))
                .is_err()
            {
                continue;
            }
            let is_it =
                |f: &String| f.starts_with(&format!("{}: ", gap.id)) && f.contains(gap.says);
            if self.failures.iter().any(is_it) {
                self.failures.retain(|f| !is_it(f));
            } else {
                stale.push(format!(
                    "{}: the known gap ({}) no longer occurs, so delete it from the list",
                    gap.id, gap.why
                ));
            }
        }
        if stale.is_empty() {
            Ok(self)
        } else {
            Err(stale.join("\n"))
        }
    }

    /// `Ok` when the law held for every document, enough documents were checked, the skips are few, and the deadline left
    /// none of the sweep's documents unreached. `at_least` is for the whole set; a shard of `n` needs its share, `at_least / n`
    /// rounded up, and must hold at least half of its share of the set, so that a hash that puts every document in one shard fails
    /// the others.
    pub fn verdict(&self, at_least: usize) -> Result<(), String> {
        let mut problems = Vec::new();
        if let Some(reach) = self.reach() {
            problems.push(reach);
        }
        let count = self.shard.map_or(1, |s| s.count as usize);
        let need = at_least.div_ceil(count);
        if self.only && self.checked == 0 {
            problems.push(format!(
                "{ENV_ONLY} names no document of the set that opens"
            ));
        } else if !self.only && self.checked + self.not_reached < need {
            problems.push(format!(
                "only {} documents were checked, {need} expected",
                self.checked
            ));
        }
        if let Some(shard) = self.shard
            && !self.only
            && self.chosen * 2 * count < self.in_set
        {
            problems.push(format!(
                "the shard {shard} holds {} of the {} documents of the set, under half of its share",
                self.chosen, self.in_set
            ));
        }
        if self.skipped.len() * 100 > self.checked + self.skipped.len() {
            problems.push(format!(
                "{} documents were skipped as malformed input, over one in a hundred: {}",
                self.skipped.len(),
                self.skipped.join(", ")
            ));
        }
        if !self.failures.is_empty() {
            problems.push(format!(
                "{} failures:\n{}",
                self.failures.len(),
                self.failures.join("\n")
            ));
        }
        if let Some(slow) = self.slow() {
            problems.push(slow);
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n"))
        }
    }

    /// A sentence when documents ran over the time limit, for a test that judges the rest itself.
    pub fn slow(&self) -> Option<String> {
        (!self.over_limit.is_empty()).then(|| {
            format!(
                "{} documents ran over the time limit:\n{}",
                self.over_limit.len(),
                self.over_limit.join("\n")
            )
        })
    }

    /// A sentence when the deadline left documents of the sweep unreached, for a test that judges the rest itself.
    pub fn reach(&self) -> Option<String> {
        (self.not_reached > 0).then(|| {
            format!(
                "{} of the {} documents of the sweep were not reached before the deadline ({ENV_DEADLINE}): the run needs more \
                 shards or more time",
                self.not_reached, self.chosen
            )
        })
    }
}

/// Picks the documents of a sweep: a comma-separated list of pieces of document ids (`<source>:<path in the source>`). When it is
/// set, a sweep runs only the documents whose id holds one of the pieces, so that a failure that a nightly run found runs again in
/// seconds with the seed it printed.
pub const ENV_ONLY: &str = "STETDOC_ONLY";
/// Splits a sweep: `i/n` runs the `i`th of `n` shards ([`Shard`]).
pub const ENV_SHARD: &str = "STETDOC_SHARD";
/// The time, in Unix seconds, after which no worker takes a new document.
pub const ENV_DEADLINE: &str = "STETDOC_DEADLINE";
/// Seconds after which the watchdog names a document that is still running.
pub const ENV_WATCH: &str = "STETDOC_WATCH_SECS";
/// Seconds over which a document is a failure of the sweep.
pub const ENV_LIMIT: &str = "STETDOC_LIMIT_SECS";
/// Seconds between two progress lines.
pub const ENV_PROGRESS: &str = "STETDOC_PROGRESS_SECS";

/// The watchdog's default. The slowest documents of the full corpora take about a minute of 50 random batches on a runner of
/// ubuntu-latest (measured in October 2026: about 20 seconds of an Apple M-series core, which does about three times the work of a
/// runner's vCPU), so a document still running after two minutes is worth a line in the log.
pub const WATCH: Duration = Duration::from_secs(120);
/// The hard limit's default: ten times the slowest documents measured, so that no document of the corpora comes near it on a
/// runner or on a busy laptop, and one that passes it has a performance bug (twelve seconds a batch).
pub const LIMIT: Duration = Duration::from_secs(600);
/// The default time between two progress lines.
pub const PROGRESS: Duration = Duration::from_secs(180);

/// The pieces of `STETDOC_ONLY`, or `None` when it is unset or empty.
fn only() -> Option<Vec<String>> {
    let pieces: Vec<String> = std::env::var(ENV_ONLY)
        .ok()?
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    (!pieces.is_empty()).then_some(pieces)
}

/// One of `count` shards of a set. A document is in shard `index` when the first 16 hexadecimal digits of its SHA-256, as a
/// number, are `index` modulo `count`: the shard is known from `INDEX.tsv` before the document is read, it is the same on every
/// night and every machine, and the same bytes are in the same shard under any name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shard {
    /// Which shard, from 0.
    pub index: u64,
    /// How many shards.
    pub count: u64,
}

impl Shard {
    /// Reads `i/n`, such as `3/16`, with `i` below `n`.
    pub fn parse(text: &str) -> Result<Shard, String> {
        let parsed = text.split_once('/').and_then(|(i, n)| {
            Some((i.trim().parse::<u64>().ok()?, n.trim().parse::<u64>().ok()?))
        });
        match parsed {
            Some((index, count)) if index < count => Ok(Shard { index, count }),
            _ => Err(format!(
                "{ENV_SHARD} is `i/n` with i below n, such as 3/16, not {text:?}"
            )),
        }
    }

    /// Whether the document with this SHA-256 (lower-case hexadecimal) is in the shard.
    pub fn holds(&self, sha256: &str) -> bool {
        shard_key(sha256) % self.count == self.index
    }
}

impl std::fmt::Display for Shard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.index, self.count)
    }
}

/// The number a document's shard is taken from: the first 16 hexadecimal digits of its SHA-256, or a hash of the text when
/// those are not hexadecimal.
pub fn shard_key(sha256: &str) -> u64 {
    sha256
        .get(..16)
        .and_then(|head| u64::from_str_radix(head, 16).ok())
        .unwrap_or_else(|| super::generate::fnv(sha256.as_bytes()))
}

/// How a sweep runs: its shard, its deadline, its watchdog and limit, and how often it says how far it got.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The shard, or `None` for every document.
    pub shard: Option<Shard>,
    /// After it, no worker takes a new document.
    pub deadline: Option<SystemTime>,
    /// A document still running after this is named in the log.
    pub watch: Duration,
    /// A document that runs longer than this is a failure.
    pub limit: Duration,
    /// The time between two progress lines.
    pub progress: Duration,
    /// How many documents run at once.
    pub workers: usize,
    /// Whether the law is a survey that most documents must pass, so that a document it does not hold for is a miss: said in the
    /// log as one (`Said::Miss`), listed in `failures` for the test to count, and not written to the run page.
    pub misses: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            shard: None,
            deadline: None,
            watch: WATCH,
            limit: LIMIT,
            progress: PROGRESS,
            workers: std::thread::available_parallelism().map_or(4, |n| n.get()),
            misses: false,
        }
    }
}

impl Settings {
    /// The settings that the environment names, with the defaults for the rest.
    pub fn from_env() -> Result<Settings, String> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let secs = |name: &str, default: Duration| -> Result<Duration, String> {
            var(name).map_or(Ok(default), |v| {
                v.trim()
                    .parse::<u64>()
                    .map(Duration::from_secs)
                    .map_err(|_| format!("{name} is a number of seconds, not {v:?}"))
            })
        };
        let deadline = match var(ENV_DEADLINE) {
            None => None,
            Some(v) => Some(
                v.trim()
                    .parse::<u64>()
                    .map(|s| UNIX_EPOCH + Duration::from_secs(s))
                    .map_err(|_| format!("{ENV_DEADLINE} is a time in Unix seconds, not {v:?}"))?,
            ),
        };
        Ok(Settings {
            shard: var(ENV_SHARD).map(|v| Shard::parse(&v)).transpose()?,
            deadline,
            watch: secs(ENV_WATCH, WATCH)?,
            limit: secs(ENV_LIMIT, LIMIT)?,
            progress: secs(ENV_PROGRESS, PROGRESS)?,
            ..Settings::default()
        })
    }
}

/// How a sweep over the corpus speaks: each line to stderr at once (with `--nocapture`, so the log of a job that is cancelled
/// still has it), and the failures, the deadline and the summary also to the run page ([`to_the_summary`]).
pub fn to_the_log(said: Said, line: &str) {
    let _ = writeln!(std::io::stderr(), "laws: {line}");
    if matches!(said, Said::Failure | Said::Deadline | Said::Summary) {
        to_the_summary(line);
    }
}

/// Adds a block of text to the summary of the run page (`GITHUB_STEP_SUMMARY`), when there is one: on GitHub Actions. A line is
/// cut at 4,000 characters, since the summary holds at most 1 MiB.
pub fn to_the_summary(text: &str) {
    if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY")
        && let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path)
    {
        let text: String = text.chars().take(4000).collect();
        let _ = writeln!(file, "~~~text\n{text}\n~~~");
    }
}

/// Runs `law` over the documents of the corpus set that must open, of this shard, or those of them that `STETDOC_ONLY` names,
/// and says what it finds as it goes ([`to_the_log`]). `name` names the sweep in the log. `None` when `STETDOC_CORPUS_DIR` is not
/// set.
pub fn over_the_corpus(
    name: &str,
    law: impl Fn(&[u8]) -> Result<(), String> + Sync,
) -> Option<Result<Swept, String>> {
    let corpus = match Corpus::from_env()? {
        Ok(corpus) => corpus,
        Err(e) => return Some(Err(e)),
    };
    let settings = match Settings::from_env() {
        Ok(s) => s,
        Err(e) => return Some(Err(e)),
    };
    Some(Ok(over(&corpus, name, &settings, &to_the_log, law)))
}

/// [`over_the_corpus`] for a law that most documents must pass, such as L2 by block: a document it does not hold for is a miss,
/// said in the log as one and not as a failure, and `failures` lists the misses for the test to count.
pub fn survey_of_the_corpus(
    name: &str,
    law: impl Fn(&[u8]) -> Result<(), String> + Sync,
) -> Option<Result<Swept, String>> {
    let corpus = match Corpus::from_env()? {
        Ok(corpus) => corpus,
        Err(e) => return Some(Err(e)),
    };
    let settings = match Settings::from_env() {
        Ok(s) => Settings { misses: true, ..s },
        Err(e) => return Some(Err(e)),
    };
    Some(Ok(over(&corpus, name, &settings, &to_the_log, law)))
}

/// [`over_the_corpus`] on a corpus that is already read, with its settings and its report.
pub fn over(
    corpus: &Corpus,
    name: &str,
    settings: &Settings,
    say: &(dyn Fn(Said, &str) + Sync),
    law: impl Fn(&[u8]) -> Result<(), String> + Sync,
) -> Swept {
    let only = only();
    let expected: Vec<&Entry> = corpus.expected_ok().collect();
    let entries: Vec<&Entry> = expected
        .iter()
        .copied()
        .filter(|e| settings.shard.is_none_or(|s| s.holds(&e.sha256)))
        .filter(|e| {
            only.as_ref()
                .is_none_or(|pieces| pieces.iter().any(|p| e.id.contains(p.as_str())))
        })
        .collect();
    let mut swept = sweep(
        name,
        &entries,
        |e| e.id.as_str(),
        settings,
        say,
        |entry| {
            let bytes = match std::fs::read(&entry.path) {
                Ok(b) => b,
                Err(e) => {
                    return Outcome::Failed(format!("cannot read {}: {e}", entry.path.display()));
                }
            };
            match Document::open(bytes.clone(), &OpenOptions::default()) {
                Ok(_) => Outcome::Checked(law(&bytes)),
                Err(e) if e.code() == ErrorCode::MalformedInput => Outcome::Skipped,
                Err(e) => Outcome::Failed(format!("open: {e}")),
            }
        },
    );
    swept.only = only.is_some();
    swept.shard = settings.shard;
    swept.in_set = expected.len();
    swept
}
