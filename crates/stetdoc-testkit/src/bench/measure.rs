//! The time and the peak memory of one benchmark on one input.
//!
//! **Time** is the median of several runs, after one that is not counted: as many runs as fit in
//! about a second, and between three and twenty-one. Each run starts from a fresh `prepare`, and
//! only the call of the prepared run is on the clock.
//!
//! **Peak memory** is the most the program had allocated at any moment of one more run, above what
//! it had allocated before `prepare`, so the document that the run holds counts as it does for a
//! caller that holds one. The count comes from a counting global allocator that the benchmark
//! binary installs (`stats_alloc`), and is the bytes the allocator was asked for, not the resident
//! set of the process. The resident set includes the allocator's slack, the stack and whatever the
//! system maps for the binary, cannot be reset between benchmarks on every platform, and moves with
//! the allocator's habits, so a budget on it cannot be checked closely and a regression of ten per
//! cent cannot be told from noise. The count can: it is the same on every run of the same code.
//!
//! The count is of requested bytes, with `realloc` counted by what it adds, which is what a
//! system allocator that grows a block in place does. A counter that copies on `realloc` (such as
//! `peak_alloc`) adds the old block to the new one while a vector doubles, and put parsing the
//! densest 300-page contract at 126% of its budget where this counts 92%.
//!
//! A counter that only adds and subtracts cannot say when the total was highest, so a thread reads
//! it in a spin loop while a run goes on, and the peak is the highest reading, of three runs, and
//! of the count when the run returns, which is the peak of anything that builds a structure and
//! hands it back. A spike that lasts less than a reading is missed; a reading takes about a
//! microsecond and a block of a mebibyte takes a hundred to fill, so a miss is a rounding error.
//! A machine so loaded that the reading thread is not scheduled for a while can miss more, which is
//! why there are three runs, and why each run counts its readings ([`Watched`]): the harness warns
//! when a long run got few of them. Timing runs have no such thread, and the counting costs them
//! two atomic additions for each allocation.

use super::spec::{Bench, Input};
use stats_alloc::INSTRUMENTED_SYSTEM as ALLOC;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How closely the heap was watched during one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watched {
    /// How many times the sampler read the heap while the run went on.
    pub readings: u64,
    /// How long the run lasted.
    pub over: Duration,
}

impl Watched {
    /// Readings in a millisecond of the run, on average.
    pub(crate) fn per_ms(&self) -> f64 {
        self.readings as f64 / (self.over.as_secs_f64() * 1000.0).max(1e-6)
    }
}

/// What one benchmark did on one input.
#[derive(Clone, Copy, Debug)]
pub struct Measured {
    /// The median time of a run.
    pub median: Duration,
    /// The fastest run.
    pub min: Duration,
    /// The slowest run.
    pub max: Duration,
    /// How many runs were timed.
    pub runs: usize,
    /// The peak memory of the run that was measured for it, in bytes.
    pub peak: u64,
    /// How closely the heap was watched in the run that was watched the least closely of the three
    /// that the peak is taken from.
    pub watched: Watched,
}

/// The bytes allocated and not yet freed.
fn live() -> usize {
    let s = ALLOC.stats();
    s.bytes_allocated.saturating_sub(s.bytes_deallocated)
}

/// Whether this binary counts allocations: the benchmark binary has to install
/// `stats_alloc::INSTRUMENTED_SYSTEM` as its global allocator.
pub(crate) fn counting_allocator_is_installed() -> bool {
    let before = ALLOC.stats().bytes_allocated;
    let probe = std::hint::black_box(vec![0u8; 1 << 20]);
    let after = ALLOC.stats().bytes_allocated;
    drop(probe);
    after.saturating_sub(before) >= 1 << 20
}

/// The median of the times: the middle one, or the mean of the two in the middle.
pub(crate) fn median(times: &[Duration]) -> Duration {
    let mut sorted = times.to_vec();
    sorted.sort();
    match sorted.len() {
        0 => Duration::ZERO,
        n if n % 2 == 1 => sorted[n / 2],
        n => (sorted[n / 2 - 1] + sorted[n / 2]) / 2,
    }
}

/// One run for its time. What the run keeps is dropped after the clock stops.
fn timed(bench: &Bench, input: &Input) -> Result<Duration, String> {
    let run = (bench.prepare)(input)?;
    let started = Instant::now();
    let kept = run()?;
    let took = started.elapsed();
    drop(kept);
    Ok(took)
}

/// One run for its peak memory, read by a thread that spins on the count while the run goes on.
/// The run starts when the thread has taken its first reading, so that none of it is unwatched.
fn peak_of(bench: &Bench, input: &Input) -> Result<(u64, Watched), String> {
    let base = live();
    let run = (bench.prepare)(input)?;
    let top = Arc::new(AtomicUsize::new(live()));
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(AtomicBool::new(false));
    let watcher = {
        let (top, stop, ready) = (Arc::clone(&top), Arc::clone(&stop), Arc::clone(&ready));
        std::thread::Builder::new()
            .name("bench-sampler".to_owned())
            .spawn(move || {
                let mut readings = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    top.fetch_max(live(), Ordering::Relaxed);
                    readings += 1;
                    if readings == 1 {
                        ready.store(true, Ordering::Release);
                    }
                    std::hint::spin_loop();
                }
                readings
            })
            .map_err(|e| format!("cannot start the thread that watches the heap: {e}"))?
    };
    while !ready.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    let started = Instant::now();
    let done = run();
    let over = started.elapsed();
    top.fetch_max(live(), Ordering::Relaxed);
    stop.store(true, Ordering::Relaxed);
    let readings = watcher.join().unwrap_or(0);
    let kept = done?;
    drop(kept);
    let peak = top.load(Ordering::Relaxed).saturating_sub(base) as u64;
    Ok((peak, Watched { readings, over }))
}

/// Measures a benchmark on an input: the median and the fastest of several timed runs, and the peak
/// memory of one more, counted by the allocator that the binary installs
/// (`stats_alloc::INSTRUMENTED_SYSTEM`) and read by a spinning thread, the highest of three runs.
/// The error names what failed, or says that the allocator is not counting.
pub fn measure(bench: &Bench, input: &Input) -> Result<Measured, String> {
    if !counting_allocator_is_installed() {
        return Err("the counting allocator is not installed in this binary".to_owned());
    }
    let warm = timed(bench, input)?;
    let runs = ((1.0 / warm.as_secs_f64().max(1e-6)).ceil() as usize).clamp(3, 21);
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        times.push(timed(bench, input)?);
    }
    let (mut peak, mut watched) = peak_of(bench, input)?;
    for _ in 1..3 {
        let (next, seen) = peak_of(bench, input)?;
        peak = peak.max(next);
        if seen.per_ms() < watched.per_ms() {
            watched = seen;
        }
    }
    Ok(Measured {
        median: median(&times),
        min: times.iter().copied().min().unwrap_or_default(),
        max: times.iter().copied().max().unwrap_or_default(),
        runs,
        peak,
        watched,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::spec::{Inputs, Limit, Parses};

    fn toy(prepare: fn(&Input) -> Result<crate::bench::spec::Run, String>) -> Bench {
        Bench {
            name: "toy",
            limit: Limit::None,
            parses: Parses::Nothing,
            inputs: Inputs::Documents,
            prepare,
        }
    }

    #[test]
    fn the_median_is_the_middle_time() {
        let ms = Duration::from_millis;
        assert_eq!(median(&[ms(9), ms(1), ms(5)]), ms(5));
        assert_eq!(median(&[ms(2), ms(4), ms(8), ms(100)]), ms(6));
        assert_eq!(median(&[]), Duration::ZERO);
    }

    /// The count is the whole program's, and the other tests of this binary allocate and free while
    /// this one runs, so no figure of memory can be asserted here: `tests/bench_peak.rs` has a
    /// binary of its own and checks them. This checks the statistics of the times.
    #[test]
    fn the_runs_are_timed_and_ordered() {
        let sleeper = toy(|_| {
            Ok(Box::new(|| {
                std::thread::sleep(Duration::from_millis(2));
                Ok(Box::new(()) as crate::bench::spec::Kept)
            }))
        });
        let m = measure(&sleeper, &Input::none()).unwrap();
        assert!(m.runs >= 3 && m.min <= m.median && m.median <= m.max);
        assert!(m.min >= Duration::from_millis(2), "{:?}", m.min);
    }

    #[test]
    fn the_sampler_counts_its_readings_and_a_run_is_watched_for_as_long_as_it_lasts() {
        let sleeper = toy(|_| {
            Ok(Box::new(|| {
                std::thread::sleep(Duration::from_millis(5));
                Ok(Box::new(()) as crate::bench::spec::Kept)
            }))
        });
        let m = measure(&sleeper, &Input::none()).unwrap();
        assert!(m.watched.readings >= 1, "{:?}", m.watched);
        assert!(
            m.watched.over >= Duration::from_millis(5),
            "{:?}",
            m.watched
        );
    }

    #[test]
    fn readings_are_counted_by_the_millisecond_of_the_run() {
        let w = Watched {
            readings: 500,
            over: Duration::from_millis(50),
        };
        assert!((w.per_ms() - 10.0).abs() < 1e-9);
        let none = Watched {
            readings: 0,
            over: Duration::ZERO,
        };
        assert_eq!(none.per_ms(), 0.0, "no division by nothing");
    }

    #[test]
    fn a_failing_run_says_why() {
        let broken = toy(|_| Ok(Box::new(|| Err("the part would not parse".to_owned()))));
        assert!(
            measure(&broken, &Input::none())
                .unwrap_err()
                .contains("would not parse")
        );
        let unprepared = toy(|_| Err("no input".to_owned()));
        assert_eq!(
            measure(&unprepared, &Input::none()).unwrap_err(),
            "no input"
        );
    }

    #[test]
    fn the_counting_allocator_is_noticed() {
        assert!(counting_allocator_is_installed());
    }
}
