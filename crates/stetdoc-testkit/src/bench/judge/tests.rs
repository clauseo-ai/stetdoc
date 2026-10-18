//! Tests of the judge: budgets, known gaps, warnings, regressions and the table.

use super::*;
use crate::bench::baseline::Row;
use crate::bench::budget::{Gap, Mode, memory_gap};
use crate::bench::measure::Watched;
use crate::bench::spec::{Inputs, Limit, Op, Parses};

/// The known gaps that these tests judge against. A test says what a gap does with gaps of its own, not the real list:
/// `xml_serialize` on the two densest inputs, which were over their
/// memory budgets until the tree of a part was stored in chunks.
const KNOWN: [Gap; 2] = [
    Gap {
        bench: "xml_serialize",
        input: "dense-contract-300p",
        why: "the span tree of the densest real document and the part it writes come to 103% of the budget",
    },
    Gap {
        bench: "xml_serialize",
        input: "node-dense-18mb",
        why: "the span tree of an 18 MB body of short runs and the part it writes come to 103% of the budget",
    },
];

/// `super::judge` with `KNOWN` for the known gaps.
fn judge(bench: &Bench, input: &Input, m: &Measured, args: &Args, base: Base<'_>) -> Line {
    judge_with(&KNOWN, bench, input, m, args, base)
}

fn bench_named(name: &'static str, limit: Limit) -> Bench {
    Bench {
        name,
        limit,
        parses: Parses::Main,
        inputs: Inputs::Documents,
        prepare: |_| Err(String::new()),
    }
}

fn bench(limit: Limit) -> Bench {
    bench_named("toy", limit)
}

/// A document with a budget of 21 MiB: 4, the 1 MiB file and eight times a 2 MiB main part.
fn input_named(name: &'static str) -> Input {
    Input {
        name,
        docx: vec![0; MIB as usize],
        main_part: vec![0; 2 * MIB as usize],
        xml_bytes: 3 * MIB,
    }
}

fn input() -> Input {
    input_named("doc")
}

/// A run that the sampler read plenty of times.
fn measured(ms: u64, peak: u64) -> Measured {
    let t = Duration::from_millis(ms);
    Measured {
        median: t,
        min: t,
        max: t,
        runs: 3,
        peak,
        watched: Watched {
            readings: 1_000_000,
            over: t,
        },
    }
}

fn args(mode: Mode) -> Args {
    Args {
        mode,
        ..Args::default()
    }
}

fn strict() -> Args {
    Args {
        strict_time: true,
        ..Args::default()
    }
}

fn both(row: &Row) -> Base<'_> {
    Base {
        time: Some(row),
        peak: Some(row),
    }
}

fn base(time_ms: f64, peak: u64) -> Row {
    Row {
        bench: "toy".into(),
        input: "doc".into(),
        platform: "p".into(),
        time_ms,
        peak,
    }
}

#[test]
fn a_run_inside_its_budgets_passes_and_one_outside_them_says_which() {
    let open = bench(Limit::Op(Op::Open));
    let nightly = args(Mode::Nightly);
    let ok = judge(
        &open,
        &input(),
        &measured(120, 10 * MIB),
        &nightly,
        Base::default(),
    );
    assert!(ok.problems.is_empty() && ok.warnings.is_empty(), "{ok:?}");
    assert_eq!(ok.peak_limit, Some(4 * MIB + MIB + 16 * MIB));
    let slow = judge(
        &open,
        &input(),
        &measured(301, 10 * MIB),
        &nightly,
        Base::default(),
    );
    assert!(
        slow.problems[0].contains("over its budget of 300 ms"),
        "{:?}",
        slow.problems
    );
    let fat = judge(
        &open,
        &input(),
        &measured(1, 22 * MIB),
        &nightly,
        Base::default(),
    );
    assert!(
        fat.problems[0].contains("over the memory budget of 21.0 MiB"),
        "{:?}",
        fat.problems
    );
}

#[test]
fn two_inputs_have_twice_the_memory_budget_and_no_time_budget() {
    let compare = bench(Limit::TwoInputs);
    let line = judge(
        &compare,
        &input(),
        &measured(60_000, 42 * MIB),
        &args(Mode::Nightly),
        Base::default(),
    );
    assert!(line.problems.is_empty(), "{:?}", line.problems);
    let fat = judge(
        &compare,
        &input(),
        &measured(10, 42 * MIB + 1),
        &args(Mode::Nightly),
        Base::default(),
    );
    assert_eq!(fat.problems.len(), 1, "{:?}", fat.problems);
    assert!(
        fat.problems[0].contains("over the memory budget of 42.0 MiB"),
        "{:?}",
        fat.problems
    );
}

#[test]
fn a_known_gap_is_a_note_for_its_own_benchmark_and_input_and_nothing_else_is_exempt() {
    let nightly = args(Mode::Nightly);
    let save = bench_named("xml_serialize", Limit::Op(Op::Save));
    let dense = input_named("dense-contract-300p");
    let line = judge(
        &save,
        &dense,
        &measured(100, 25 * MIB),
        &nightly,
        Base::default(),
    );
    assert!(line.problems.is_empty(), "{:?}", line.problems);
    assert!(
        line.notes[0].contains("over the memory budget of 21.0 MiB, a known gap: "),
        "{:?}",
        line.notes
    );
    let text = table(&[line]);
    assert!(text.lines().nth(1).unwrap().ends_with("over"), "{text}");
    assert!(
        text.contains("note xml_serialize on dense-contract-300p:"),
        "{text}"
    );
    // The same benchmark on another input, and another benchmark on the same input, are over.
    let over =
        |b: &Bench, i: &Input| judge(b, i, &measured(100, 25 * MIB), &nightly, Base::default());
    let elsewhere = over(&save, &input_named("contract-300p"));
    assert!(
        elsewhere.problems[0].contains("over the memory budget"),
        "{elsewhere:?}"
    );
    let parse = bench_named("xml_parse", Limit::Op(Op::Open));
    let another = over(&parse, &dense);
    assert!(
        another.problems[0].contains("over the memory budget"),
        "{another:?}"
    );
    // Its time budget and its growth hold too.
    let slow = judge(
        &save,
        &dense,
        &measured(1_500, MIB),
        &nightly,
        Base::default(),
    );
    assert!(
        slow.problems[0].contains("over its budget of 1000 ms"),
        "{slow:?}"
    );
    let mut row = base(100.0, 20 * MIB);
    (row.bench, row.input) = ("xml_serialize".into(), "dense-contract-300p".into());
    let grown = judge(
        &save,
        &dense,
        &measured(100, 25 * MIB),
        &nightly,
        both(&row),
    );
    assert!(
        grown.problems[0].contains("1.25 times the memory"),
        "{grown:?}"
    );
}

/// The cue that ends the exemption. A benchmark that is over its memory budget on an input is a
/// known gap, in `MEMORY_GAPS`, and the benchmark run measures it every time. When something makes
/// it fit the run fails, with the text asserted here, and the fix is to delete the entry, which
/// makes the input a hard gate for the benchmark.
#[test]
fn a_known_gap_that_has_closed_fails_and_says_to_make_the_input_a_hard_gate() {
    assert!(
        memory_gap(&KNOWN, "xml_serialize", "dense-contract-300p").is_some(),
        "the gap of these tests"
    );
    let save = bench_named("xml_serialize", Limit::Op(Op::Save));
    let dense = input_named("dense-contract-300p");
    for mode in [Mode::Nightly, Mode::Ci] {
        let fits = judge(
            &save,
            &dense,
            &measured(100, 20 * MIB),
            &args(mode),
            Base::default(),
        );
        let why = &fits.problems[0];
        assert!(
            why.contains("within the memory budget of 21.0 MiB that it was known to exceed")
                && why.contains("the gap is closed")
                && why.contains(
                    "delete the entry for xml_serialize on dense-contract-300p from MEMORY_GAPS"
                )
                && why.contains("hard gate"),
            "{why}"
        );
        assert!(fits.notes.is_empty());
    }
}

#[test]
fn a_time_regression_is_a_warning_unless_the_run_is_strict_and_the_budgets_are_failures() {
    let free = bench(Limit::None);
    let nightly = args(Mode::Nightly);
    let slow = judge(
        &free,
        &input(),
        &measured(40, MIB),
        &nightly,
        both(&base(20.0, MIB)),
    );
    assert!(slow.problems.is_empty(), "{:?}", slow.problems);
    assert!(
        slow.warnings[0].contains("2.00 times slower")
            && slow.warnings[0].contains("--strict-time"),
        "{:?}",
        slow.warnings
    );
    assert!(table(&[slow]).lines().nth(1).unwrap().ends_with("warn"));
    let refused = judge(
        &free,
        &input(),
        &measured(40, MIB),
        &strict(),
        both(&base(20.0, MIB)),
    );
    assert!(
        refused.problems[0].contains("2.00 times slower"),
        "{:?}",
        refused.problems
    );
    assert!(refused.warnings.is_empty());
    let tiny = judge(
        &free,
        &input(),
        &measured(2, MIB),
        &strict(),
        both(&base(0.5, MIB)),
    );
    assert!(
        tiny.problems.is_empty() && tiny.warnings.is_empty(),
        "a difference under 2 ms is noise: {tiny:?}"
    );
    let ci = judge(
        &free,
        &input(),
        &measured(40, MIB),
        &args(Mode::Ci),
        both(&base(20.0, MIB)),
    );
    assert!(
        ci.problems.is_empty() && ci.warnings.is_empty(),
        "twice is inside the margin of a CI run"
    );
    // A budget is not a regression: it fails whatever the mode and the strictness.
    let open = bench(Limit::Op(Op::Open));
    let over = judge(
        &open,
        &input(),
        &measured(400, MIB),
        &nightly,
        Base::default(),
    );
    assert!(
        over.problems[0].contains("over its budget of 300 ms"),
        "{:?}",
        over.problems
    );
}

#[test]
fn a_ci_run_has_twice_the_time_budget_and_a_run_with_no_limit_has_only_the_baseline() {
    let open = bench(Limit::Op(Op::Open));
    let ci = judge(
        &open,
        &input(),
        &measured(500, MIB),
        &args(Mode::Ci),
        Base::default(),
    );
    assert!(ci.problems.is_empty());
    let free = judge(
        &bench(Limit::None),
        &input(),
        &measured(99_999, 99 * MIB),
        &args(Mode::Nightly),
        Base::default(),
    );
    assert!(free.problems.is_empty() && free.time_limit.is_none() && free.peak_limit.is_none());
}

#[test]
fn a_memory_regression_fails_in_both_modes_with_the_margin_of_the_mode() {
    let free = bench(Limit::None);
    let (nightly, ci) = (args(Mode::Nightly), args(Mode::Ci));
    let heavy = judge(
        &free,
        &input(),
        &measured(20, 3 * MIB),
        &ci,
        both(&base(20.0, 2 * MIB)),
    );
    assert!(
        heavy.problems[0].contains("1.50 times the memory")
            && heavy.problems[0].contains("at most 1.25 times"),
        "{:?}",
        heavy.problems
    );
    let a_little_more = measured(20, 2 * MIB + MIB / 3);
    let at_night = judge(
        &free,
        &input(),
        &a_little_more,
        &nightly,
        both(&base(20.0, 2 * MIB)),
    );
    assert!(
        at_night.problems[0].contains("times the memory")
            && at_night.problems[0].contains("at most 1.10 times"),
        "a sixth more is a regression at night: {:?}",
        at_night.problems
    );
    let in_ci = judge(
        &free,
        &input(),
        &a_little_more,
        &ci,
        both(&base(20.0, 2 * MIB)),
    );
    assert!(
        in_ci.problems.is_empty(),
        "but not in CI: {:?}",
        in_ci.problems
    );
    let same = judge(
        &free,
        &input(),
        &measured(20, 2 * MIB + 1000),
        &nightly,
        both(&base(20.0, 2 * MIB)),
    );
    assert!(same.problems.is_empty());
}

#[test]
fn a_long_run_that_the_sampler_read_few_times_is_a_warning_and_a_short_or_well_read_one_is_not() {
    let open = bench(Limit::Op(Op::Open));
    let nightly = args(Mode::Nightly);
    let watched = |readings, ms| Measured {
        watched: Watched {
            readings,
            over: Duration::from_millis(ms),
        },
        ..measured(100, 10 * MIB)
    };
    let thin = judge(
        &open,
        &input(),
        &watched(100, 200),
        &nightly,
        Base::default(),
    );
    assert!(thin.problems.is_empty());
    assert!(
        thin.warnings[0].contains("read 100 times in a run of 200 ms")
            && thin.warnings[0].contains("may be too low"),
        "{:?}",
        thin.warnings
    );
    let unread = judge(&open, &input(), &watched(0, 200), &nightly, Base::default());
    assert!(
        unread.warnings[0].contains("read 0 times"),
        "{:?}",
        unread.warnings
    );
    let short = judge(&open, &input(), &watched(1, 5), &nightly, Base::default());
    assert!(
        short.warnings.is_empty(),
        "a run of 5 ms is too short to ask: {:?}",
        short.warnings
    );
    let well = judge(
        &open,
        &input(),
        &watched(4_000, 200),
        &nightly,
        Base::default(),
    );
    assert!(
        well.warnings.is_empty(),
        "20 a millisecond is enough: {:?}",
        well.warnings
    );
}

#[test]
fn times_are_shown_with_the_digits_that_mean_something() {
    let shown = |us: u64| millis(Duration::from_micros(us));
    assert_eq!(shown(5), "0.005");
    assert_eq!(shown(480), "0.48");
    assert_eq!(shown(12_340), "12.3");
    assert_eq!(shown(456_000), "456");
}

#[test]
fn the_table_aligns_and_names_what_failed_warned_and_is_known() {
    let open = bench(Limit::Op(Op::Open));
    let nightly = args(Mode::Nightly);
    let save = bench_named("xml_serialize", Limit::Op(Op::Save));
    let lines = vec![
        judge(
            &open,
            &input(),
            &measured(120, 10 * MIB),
            &nightly,
            both(&base(100.0, 10 * MIB)),
        ),
        judge(
            &open,
            &input(),
            &measured(400, 10 * MIB),
            &nightly,
            Base::default(),
        ),
        judge(
            &open,
            &input(),
            &measured(200, 10 * MIB),
            &nightly,
            both(&base(100.0, 10 * MIB)),
        ),
        judge(
            &save,
            &input_named("node-dense-18mb"),
            &measured(5, 25 * MIB),
            &nightly,
            Base::default(),
        ),
    ];
    let text = table(&lines);
    let rows: Vec<&str> = text.lines().collect();
    assert!(
        rows[0].starts_with("benchmark") && rows[0].contains("limit MiB"),
        "{text}"
    );
    assert!(
        rows[1].ends_with("ok") && rows[1].contains("1.20"),
        "{text}"
    );
    assert!(rows[2].ends_with("FAIL"), "{text}");
    assert!(rows[3].ends_with("warn"), "{text}");
    assert!(rows[4].ends_with("over"), "{text}");
    let kinds: Vec<&str> = rows[5..]
        .iter()
        .map(|r| r.split(' ').next().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["FAIL", "warn", "note"],
        "failures, then warnings, then notes: {text}"
    );
    assert!(
        rows[5].starts_with("FAIL toy on doc: took 400.0 ms, over its budget of 300 ms"),
        "{text}"
    );
}
