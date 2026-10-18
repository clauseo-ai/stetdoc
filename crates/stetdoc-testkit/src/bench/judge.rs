//! Judging what was measured: the budgets, the baseline, and the table that shows both.
//!
//! What fails and what only warns is decided in `budget.rs` and applied here.

use super::args::Args;
use super::baseline::Base;
use super::budget::{
    Gap, LONG_RUN, MEMORY_GAPS, MEMORY_NOISE, MIB, MIN_READINGS_PER_MS, memory_gap, peak_limit,
    time_limit,
};
use super::measure::Measured;
use super::spec::{Bench, Input};
use std::time::Duration;

/// One row of the table: a benchmark on an input, and how it did.
#[derive(Clone, Debug)]
pub(crate) struct Line {
    /// The benchmark.
    pub bench: &'static str,
    /// The input.
    pub input: &'static str,
    /// The median time.
    pub time: Duration,
    /// The time limit in this mode, if the benchmark has one.
    pub time_limit: Option<Duration>,
    /// The peak memory in bytes.
    pub peak: u64,
    /// The memory limit in bytes, if the benchmark has one.
    pub peak_limit: Option<u64>,
    /// The time of the fastest run as a multiple of the baseline, where there is a baseline for
    /// this platform.
    pub time_vs_base: Option<f64>,
    /// The peak memory as a multiple of the baseline.
    pub peak_vs_base: Option<f64>,
    /// What fails the run, in sentences. Empty when the benchmark passes.
    pub problems: Vec<String>,
    /// What does not fail the run and is worth a line: a time regression against the baseline,
    /// and a run whose peak the sampler may have missed.
    pub warnings: Vec<String>,
    /// A memory budget that the benchmark is known to exceed on the input (`MEMORY_GAPS`).
    pub notes: Vec<String>,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / MIB as f64
}

/// Judges one measurement against the budgets of its mode and against its baseline rows. The time
/// of the fastest run is compared with the baseline, because a loaded machine only makes runs
/// slower, and the budget is held against the median, which is what a user would get.
pub(crate) fn judge(
    bench: &Bench,
    input: &Input,
    m: &Measured,
    args: &Args,
    base: Base<'_>,
) -> Line {
    judge_with(&MEMORY_GAPS, bench, input, m, args, base)
}

/// `judge` against a list of known gaps. The real list is `MEMORY_GAPS`, and a test supplies its
/// own to check what a gap does, which does not depend on there being one today.
pub(crate) fn judge_with(
    gaps: &[Gap],
    bench: &Bench,
    input: &Input,
    m: &Measured,
    args: &Args,
    base: Base<'_>,
) -> Line {
    let mode = args.mode;
    let time_limit = time_limit(bench.limit, mode);
    let peak_limit = peak_limit(
        bench.limit,
        input.docx.len() as u64,
        input.parsed(bench.parses),
    );
    let (mut problems, mut warnings, mut notes) = (Vec::new(), Vec::new(), Vec::new());
    if let Some(limit) = time_limit.filter(|l| m.median > *l) {
        problems.push(format!(
            "took {:.1} ms, over its budget of {:.0} ms",
            ms(m.median),
            ms(limit)
        ));
    }
    if let Some(limit) = peak_limit {
        let gap = memory_gap(gaps, bench.name, input.name);
        let (peak, budget) = (mib(m.peak), mib(limit));
        match (m.peak > limit, gap) {
            (true, None) => problems.push(format!(
                "peaked at {peak:.1} MiB, over the memory budget of {budget:.1} MiB"
            )),
            (true, Some(gap)) => notes.push(format!(
                "peaked at {peak:.1} MiB, over the memory budget of {budget:.1} MiB, a known gap: {}",
                gap.why
            )),
            (false, Some(gap)) => problems.push(format!(
                "peaked at {peak:.1} MiB, within the memory budget of {budget:.1} MiB that it was known to exceed ({}): the gap is closed, so delete the entry for {} on {} from MEMORY_GAPS in crates/stetdoc-testkit/src/bench/budget.rs, which makes this input a hard gate for it",
                gap.why, gap.bench, gap.input
            )),
            (false, None) => {}
        }
    }
    let time_vs_base = base.time.map(|b| ms(m.min) / b.time_ms.max(0.001));
    let peak_vs_base = base.peak.map(|b| m.peak as f64 / (b.peak.max(1)) as f64);
    if let (Some(b), Some(ratio)) = (base.time, time_vs_base) {
        let grew = ms(m.min) - b.time_ms;
        if ratio > mode.time_regression_factor() && grew > ms(mode.time_noise()) {
            let text = format!(
                "is {ratio:.2} times slower than its baseline of {:.1} ms (at most {:.1} times), by its fastest run",
                b.time_ms,
                mode.time_regression_factor()
            );
            if args.strict_time {
                problems.push(text);
            } else {
                warnings.push(format!("{text}; a warning until --strict-time"));
            }
        }
    }
    if let (Some(b), Some(ratio)) = (base.peak, peak_vs_base)
        && ratio > mode.memory_regression_factor()
        && m.peak.saturating_sub(b.peak) > MEMORY_NOISE
    {
        problems.push(format!(
            "uses {ratio:.2} times the memory of its baseline of {:.1} MiB (at most {:.2} times)",
            mib(b.peak),
            mode.memory_regression_factor()
        ));
    }
    let w = m.watched;
    if w.over >= LONG_RUN && w.per_ms() < MIN_READINGS_PER_MS as f64 {
        warnings.push(format!(
            "the heap was read {} times in a run of {:.0} ms (at least {MIN_READINGS_PER_MS} a millisecond are expected), so a short peak may have been missed and the {:.1} MiB may be too low: run it again on a quieter machine",
            w.readings,
            ms(w.over),
            mib(m.peak)
        ));
    }
    Line {
        bench: bench.name,
        input: input.name,
        time: m.median,
        time_limit,
        peak: m.peak,
        peak_limit,
        time_vs_base,
        peak_vs_base,
        problems,
        warnings,
        notes,
    }
}

fn millis(d: Duration) -> String {
    let v = ms(d);
    if v >= 100.0 {
        format!("{v:.0}")
    } else if v >= 10.0 {
        format!("{v:.1}")
    } else if v >= 0.1 {
        format!("{v:.2}")
    } else {
        format!("{v:.3}")
    }
}

fn mebibytes(bytes: u64) -> String {
    format!("{:.1}", mib(bytes))
}

/// The table, one line for each benchmark and input, aligned. The last column is `FAIL` for a line
/// that fails the run, `warn` for one that has a warning, `over` for one over a known gap, and `ok`.
/// The reasons follow the table, a line each.
pub(crate) fn table(lines: &[Line]) -> String {
    let header = [
        "benchmark",
        "input",
        "ms",
        "limit ms",
        "peak MiB",
        "limit MiB",
        "time/base",
        "mem/base",
        "",
    ];
    let mut rows: Vec<[String; 9]> = vec![header.map(str::to_owned)];
    for l in lines {
        let ratio = |r: Option<f64>| r.map_or("-".to_owned(), |r| format!("{r:.2}"));
        rows.push([
            l.bench.to_owned(),
            l.input.to_owned(),
            millis(l.time),
            l.time_limit.map_or("-".to_owned(), millis),
            mebibytes(l.peak),
            l.peak_limit.map_or("-".to_owned(), mebibytes),
            ratio(l.time_vs_base),
            ratio(l.peak_vs_base),
            if !l.problems.is_empty() {
                "FAIL".to_owned()
            } else if !l.warnings.is_empty() {
                "warn".to_owned()
            } else if !l.notes.is_empty() {
                "over".to_owned()
            } else {
                "ok".to_owned()
            },
        ]);
    }
    let widths: Vec<usize> = (0..9)
        .map(|c| rows.iter().map(|r| r[c].chars().count()).max().unwrap_or(0))
        .collect();
    let mut out = String::new();
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(c, cell)| {
                if c < 2 || c == 8 {
                    format!("{cell:<w$}", w = widths[c])
                } else {
                    format!("{cell:>w$}", w = widths[c])
                }
            })
            .collect();
        out.push_str(cells.join("  ").trim_end());
        out.push('\n');
    }
    for l in lines {
        for p in &l.problems {
            out.push_str(&format!("FAIL {} on {}: {p}\n", l.bench, l.input));
        }
    }
    for l in lines {
        for w in &l.warnings {
            out.push_str(&format!("warn {} on {}: {w}\n", l.bench, l.input));
        }
    }
    for l in lines {
        for n in &l.notes {
            out.push_str(&format!("note {} on {}: {n}\n", l.bench, l.input));
        }
    }
    out
}

#[cfg(test)]
mod tests;
