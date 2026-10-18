//! Running the benchmarks: measuring each on its inputs, judging it, and reporting.

use super::args::Args;
use super::baseline::{self, Row};
use super::budget::{MEMORY_GAPS, check_gaps};
use super::inputs;
use super::judge::{Line, judge, table};
use super::measure::measure;
use super::spec::{Bench, Input, Inputs};
use std::path::Path;

/// What a run of the benchmarks found.
#[derive(Clone, Debug)]
pub(crate) struct Outcome {
    /// One line for each benchmark and input.
    pub lines: Vec<Line>,
    /// What was measured, as baseline rows for this platform.
    pub rows: Vec<Row>,
}

/// Runs the benchmarks that `args` asks for on `inputs`, and judges them against `baseline` rows
/// of `platform`. A benchmark that cannot run is an error that names it.
pub(crate) fn execute(
    benches: &[Bench],
    inputs: &[Input],
    args: &Args,
    baseline: &[Row],
    platform: &str,
) -> Result<Outcome, String> {
    let (mut lines, mut rows) = (Vec::new(), Vec::new());
    for bench in benches.iter().filter(|b| args.wants_bench(b.name)) {
        let on: Vec<&Input> = match bench.inputs {
            Inputs::Documents => inputs
                .iter()
                .filter(|i| i.name != inputs::NUMBERED)
                .collect(),
            Inputs::Only(name) => inputs.iter().filter(|i| i.name == name).collect(),
        };
        for input in on {
            eprintln!("measuring {} on {}", bench.name, input.name);
            let m = measure(bench, input)
                .map_err(|e| format!("{} on {}: {e}", bench.name, input.name))?;
            let base = baseline::base(baseline, bench.name, input.name, platform);
            lines.push(judge(bench, input, &m, args, base));
            rows.push(Row {
                bench: bench.name.to_owned(),
                input: input.name.to_owned(),
                platform: platform.to_owned(),
                time_ms: m.min.as_secs_f64() * 1000.0,
                peak: m.peak,
            });
        }
    }
    Ok(Outcome { lines, rows })
}

/// What a run says about itself above the table: where and how it ran, and the inputs, so that
/// the limits in the table can be read.
fn heading(built: &[Input], args: &Args, platform: &str) -> String {
    let cpus = std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get);
    let mut out = format!(
        "stetdoc benchmarks on {platform}, {cpus} cpus, {} margins{}\n",
        args.mode.name(),
        if args.strict_time {
            ", strict time"
        } else {
            ""
        }
    );
    for input in built {
        out.push_str(&inputs::describe(input));
        out.push('\n');
    }
    out
}

/// The line that sums a run up.
fn summary(lines: &[Line]) -> String {
    let failed = lines.iter().filter(|l| !l.problems.is_empty()).count();
    let warned = lines
        .iter()
        .filter(|l| l.problems.is_empty() && !l.warnings.is_empty())
        .count();
    let known = lines
        .iter()
        .filter(|l| l.problems.is_empty() && l.warnings.is_empty() && !l.notes.is_empty())
        .count();
    format!(
        "{} runs: {failed} failed, {warned} with a warning, {known} over a known memory gap",
        lines.len()
    )
}

/// What the GitHub Actions log turns into annotations on the run: a line for each failure and
/// each warning. The text is escaped as the workflow commands ask.
fn annotations(lines: &[Line]) -> String {
    let escape = |s: &str| {
        s.replace('%', "%25")
            .replace('\r', "%0D")
            .replace('\n', "%0A")
    };
    let mut out = String::new();
    for (kind, pick) in [("error", true), ("warning", false)] {
        for l in lines {
            let texts = if pick { &l.problems } else { &l.warnings };
            for t in texts {
                let text = escape(&format!("{} on {}: {t}", l.bench, l.input));
                out.push_str(&format!("::{kind} title=benchmark::{text}\n"));
            }
        }
    }
    out
}

/// Writes what a run found into a directory, which is created: the report, and the rows as the
/// baseline file has them, so that the rows of a platform can be recorded from a run of it.
fn write_out(dir: &Path, report: &str, rows: &[Row]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let table = dir.join("bench-table.txt");
    std::fs::write(&table, report).map_err(|e| format!("cannot write {}: {e}", table.display()))?;
    baseline::write_to(&dir.join("bench-rows.tsv"), rows)
}

/// Runs the benchmarks as `STETDOC_BENCH_ARGS` says: builds the inputs, measures, prints the table,
/// and fails, with the reasons, if a budget or the memory baseline is broken. With
/// `--update-baseline` it writes the baseline rows of this platform instead of judging against
/// them.
pub fn run(benches: &[Bench]) -> Result<(), String> {
    let args = Args::from_env()?;
    check_gaps(&MEMORY_GAPS, benches, &inputs::NAMES)?;
    let wanted: Vec<Bench> = benches
        .iter()
        .filter(|b| args.wants_bench(b.name))
        .copied()
        .collect();
    if args.list {
        println!("benchmarks:");
        for b in benches {
            println!("  {:<28} {:?}, {:?}", b.name, b.limit, b.inputs);
        }
        println!(
            "inputs: {}, and {} for the benchmarks that name it",
            inputs::NAMES.join(", "),
            inputs::NUMBERED
        );
        return Ok(());
    }
    if wanted.is_empty() {
        return Err("no benchmark matches --filter; --list names them".to_owned());
    }
    let needed = |name: &str| {
        wanted.iter().any(|b| match b.inputs {
            Inputs::Documents => inputs::NAMES.contains(&name),
            Inputs::Only(only) => only == name,
        })
    };
    let built = inputs::build(|n| needed(n) && args.wants_input(n))?;
    if built.is_empty() {
        return Err("no input matches --input; --list names them".to_owned());
    }
    let baseline = baseline::load()?;
    let platform = baseline::platform();
    let heading = heading(&built, &args, &platform);
    print!("{heading}");
    if !args.update_baseline && !baseline.iter().any(|r| r.platform == platform) {
        let memory = if baseline.is_empty() {
            "against the budgets only"
        } else {
            "against the budgets and the memory of another platform"
        };
        println!(
            "note: the baseline has no rows for {platform}, so time is checked against the budgets only and memory {memory}; `cargo xtask bench --update-baseline` writes the rows"
        );
    }
    let outcome = execute(&wanted, &built, &args, &baseline, &platform)?;
    let table = table(&outcome.lines);
    println!("{table}");
    let summary = summary(&outcome.lines);
    println!("{summary}");
    if let Some(dir) = &args.out {
        write_out(
            dir,
            &format!("{heading}{table}\n{summary}\n"),
            &outcome.rows,
        )?;
    }
    if std::env::var_os("GITHUB_ACTIONS").is_some() {
        print!("{}", annotations(&outcome.lines));
    }
    if args.update_baseline {
        let merged = baseline::merge(baseline, &outcome.rows);
        baseline::write_to(&baseline::path(), &merged)?;
        println!(
            "wrote {} rows for {platform} to {}",
            outcome.rows.len(),
            baseline::path().display()
        );
        return Ok(());
    }
    let failed: Vec<String> = outcome
        .lines
        .iter()
        .flat_map(|l| {
            l.problems
                .iter()
                .map(move |p| format!("{} on {}: {p}", l.bench, l.input))
        })
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!("{summary}:\n{}", failed.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::spec::{Kept, Limit, Op, Parses, Run};

    fn allocating(_: &Input) -> Result<Run, String> {
        Ok(Box::new(|| Ok(Box::new(vec![0u8; 1 << 20]) as Kept)))
    }

    fn toy(name: &'static str, limit: Limit) -> Bench {
        Bench {
            name,
            limit,
            parses: Parses::Nothing,
            inputs: Inputs::Documents,
            prepare: allocating,
        }
    }

    #[test]
    fn every_benchmark_runs_on_every_input_and_one_that_names_its_input_runs_on_that_one_only() {
        let inputs = [
            from_body_small("a"),
            from_body_small("b"),
            from_body_small(inputs::NUMBERED),
        ];
        let benches = [
            toy("one", Limit::Op(Op::Open)),
            Bench {
                inputs: Inputs::Only(inputs::NUMBERED),
                ..toy("alone", Limit::None)
            },
        ];
        let out = execute(&benches, &inputs, &Args::default(), &[], "p").unwrap();
        let names: Vec<(&str, &str)> = out.lines.iter().map(|l| (l.bench, l.input)).collect();
        assert_eq!(
            names,
            [("one", "a"), ("one", "b"), ("alone", inputs::NUMBERED)],
            "the numbered input is not a document input"
        );
        // Whether the peaks are inside the budget is not asked: the count is the whole test
        // binary's, and the other tests allocate while this one runs.
        let limited: Vec<bool> = out.lines.iter().map(|l| l.peak_limit.is_some()).collect();
        assert_eq!(limited, [true, true, false], "{:?}", out.lines);
        assert_eq!(out.rows.len(), 3);
        assert!(out.rows.iter().all(|r| r.platform == "p"));
    }

    fn from_body_small(name: &'static str) -> Input {
        inputs::from_body(name, "<w:p><w:r><w:t>x</w:t></w:r></w:p>")
    }

    #[test]
    fn a_filter_picks_benchmarks_and_a_regression_against_a_baseline_row_is_found() {
        let inputs = [from_body_small("a")];
        let benches = [
            toy("package_open", Limit::None),
            toy("xml_parse", Limit::None),
        ];
        let args = Args::parse(&["--filter", "xml"]).unwrap();
        let base = [Row {
            bench: "xml_parse".into(),
            input: "a".into(),
            platform: "p".into(),
            time_ms: 1.0,
            peak: 1 << 10,
        }];
        let out = execute(&benches, &inputs, &args, &base, "p").unwrap();
        assert_eq!(out.lines.len(), 1);
        assert!(
            out.lines[0]
                .problems
                .iter()
                .any(|p| p.contains("memory of its baseline")),
            "{:?}",
            out.lines[0].problems
        );
        let elsewhere = execute(&benches, &inputs, &args, &base, "other").unwrap();
        assert!(
            elsewhere.lines[0].time_vs_base.is_none(),
            "the time of another platform is not used"
        );
        assert!(
            elsewhere.lines[0].peak_vs_base.is_some(),
            "the memory count does not depend on the machine, so another platform's is"
        );
    }

    #[test]
    fn a_benchmark_that_cannot_run_is_named() {
        let broken = Bench {
            prepare: |_| Err("no part".to_owned()),
            ..toy("broken", Limit::None)
        };
        let err = execute(
            &[broken],
            &[from_body_small("a")],
            &Args::default(),
            &[],
            "p",
        )
        .unwrap_err();
        assert_eq!(err, "broken on a: no part");
    }

    fn line(problems: &[&str], warnings: &[&str], notes: &[&str]) -> Line {
        let strings = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
        Line {
            bench: "xml_parse",
            input: "doc",
            time: std::time::Duration::from_millis(5),
            time_limit: None,
            peak: 1 << 20,
            peak_limit: None,
            time_vs_base: None,
            peak_vs_base: None,
            problems: strings(problems),
            warnings: strings(warnings),
            notes: strings(notes),
        }
    }

    #[test]
    fn a_run_is_summed_up_by_what_failed_warned_and_is_known() {
        let lines = [
            line(&["over"], &["slow"], &[]),
            line(&[], &["slow"], &["gap"]),
            line(&[], &[], &["gap"]),
            line(&[], &[], &[]),
        ];
        assert_eq!(
            summary(&lines),
            "4 runs: 1 failed, 1 with a warning, 1 over a known memory gap"
        );
    }

    #[test]
    fn github_gets_an_annotation_for_each_failure_and_warning_with_the_text_escaped() {
        let lines = [line(
            &["peaked at 103% of it"],
            &["slow\nreally"],
            &["a note"],
        )];
        let text = annotations(&lines);
        assert_eq!(
            text,
            "::error title=benchmark::xml_parse on doc: peaked at 103%25 of it\n::warning title=benchmark::xml_parse on doc: slow%0Areally\n"
        );
    }

    #[test]
    fn the_report_and_the_rows_are_written_into_a_directory_that_is_made() {
        let dir = tempfile::tempdir().unwrap().keep().join("results");
        let rows = [Row {
            bench: "xml_parse".into(),
            input: "doc".into(),
            platform: "linux-x86_64".into(),
            time_ms: 1.5,
            peak: 2048,
        }];
        write_out(&dir, "the report\n", &rows).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("bench-table.txt")).unwrap(),
            "the report\n"
        );
        let back =
            baseline::parse(&std::fs::read_to_string(dir.join("bench-rows.tsv")).unwrap()).unwrap();
        assert_eq!(back, rows);
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_heading_says_where_and_how_the_run_was_made_and_describes_each_input() {
        let built = [from_body_small("a")];
        let args = Args::parse(&["--ci", "--strict-time"]).unwrap();
        let text = heading(&built, &args, "linux-x86_64");
        assert!(
            text.starts_with("stetdoc benchmarks on linux-x86_64, ")
                && text
                    .lines()
                    .next()
                    .unwrap()
                    .ends_with("ci margins, strict time"),
            "{text}"
        );
        assert!(text.lines().nth(1).unwrap().starts_with("a "), "{text}");
    }
}
