//! The speed and memory budgets, and how strictly a run is judged against them.
//!
//! The budgets are those of `ARCHITECTURE.md`, section Performance, for a 300-page contract on
//! linux-x64: open 300 ms, view 200 ms, a 50-operation edit 1 s, save and export 1 s each, and a
//! peak memory of 4 MiB plus the file plus eight times the XML that is parsed.
//!
//! What is a failure and what is a warning is decided here. **Failures** are the budgets (time and
//! memory), a memory regression against the baseline, and a known memory gap that has closed.
//! **Warnings** are a time regression against the baseline, which a loaded machine produces by
//! itself and which will stay a warning until there is data on how much a runner varies (`--strict-time`
//! makes it a failure), and a run that the heap sampler watched too thinly to trust its peak.

use super::spec::{Bench, Inputs, Limit, Op};
use std::time::Duration;

/// One mebibyte.
pub(crate) const MIB: u64 = 1 << 20;

/// The time budget of an operation.
pub(crate) fn op_budget(op: Op) -> Duration {
    match op {
        Op::Open => Duration::from_millis(300),
        Op::View => Duration::from_millis(200),
        Op::Edit | Op::Save | Op::Export => Duration::from_secs(1),
    }
}

/// The peak memory budget: 4 MiB, the file, and eight times the uncompressed size of the XML
/// parts that are parsed.
pub(crate) fn memory_budget(file: u64, parsed_xml: u64) -> u64 {
    4 * MIB + file + 8 * parsed_xml
}

/// How strictly a run is judged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// The numbers as written, for a developer's machine or a nightly job on a quiet one.
    #[default]
    Nightly,
    /// Generous margins for a shared runner, which is slower and noisier than the machine the
    /// budgets are written for: twice the time budgets, three times a time baseline, and a
    /// quarter more memory than a baseline. The memory budget is the same in either mode.
    Ci,
}

impl Mode {
    /// The name that a report gives it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Mode::Nightly => "nightly",
            Mode::Ci => "ci",
        }
    }

    /// What a time budget is multiplied by.
    pub(crate) fn time_budget_factor(self) -> f64 {
        match self {
            Mode::Nightly => 1.0,
            Mode::Ci => 2.0,
        }
    }

    /// How many times slower than its baseline a benchmark may be. It applies only where the
    /// baseline was measured on the same platform.
    pub(crate) fn time_regression_factor(self) -> f64 {
        match self {
            Mode::Nightly => 1.5,
            Mode::Ci => 3.0,
        }
    }

    /// A time that grows by less than this is noise, whatever the ratio.
    pub(crate) fn time_noise(self) -> Duration {
        Duration::from_millis(2)
    }

    /// How many times the memory of its baseline a benchmark may use. The count is of bytes the
    /// allocator was asked for, which does not vary from run to run, so the margin is for the
    /// differences between platforms (the baseline may be another platform's) and for the peak
    /// that the sampling thread of a loaded machine misses. The budget itself is not scaled in
    /// either mode.
    pub(crate) fn memory_regression_factor(self) -> f64 {
        match self {
            Mode::Nightly => 1.10,
            Mode::Ci => 1.25,
        }
    }
}

/// Peak memory that grows by less than this is noise, whatever the ratio.
pub(crate) const MEMORY_NOISE: u64 = 64 << 10;

/// A run is long enough to ask whether the heap sampler saw it properly when it lasts this much.
pub(crate) const LONG_RUN: Duration = Duration::from_millis(20);

/// A long run is sampled too thinly to trust its peak when the sampler read the heap fewer times
/// than this in a millisecond. A reading takes about a microsecond, so a sampler that has a core
/// of its own reads a thousand times; one that is starved by a loaded machine reads far less.
pub(crate) const MIN_READINGS_PER_MS: u64 = 20;

/// A memory budget that a benchmark is known to exceed on an input, and why. Until the cause is
/// fixed the overage is a note and not a failure, and the entry is the only exemption there is:
/// any other benchmark or input over its budget fails. The entry ends by itself: when the pair
/// is measured within its budget the run fails and says to delete the entry, which makes the
/// input a hard gate for that benchmark.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Gap {
    /// The benchmark, by its name.
    pub bench: &'static str,
    /// The input, by its name.
    pub input: &'static str,
    /// Why it is over, and what closes the gap.
    pub why: &'static str,
}

/// The known gaps. `run` checks that each names a benchmark that has a memory budget and an input
/// that it runs on.
///
/// Two, both of compare on the dense contract, whose budget is that of two inputs (`Limit::TwoInputs`,
/// design `compare.md`, section 10): it peaks 9 to 11% over it. The section names where to look first,
/// the texts and bags of every block that the alignment keeps and the staged trees of the redline.
///
/// There were four before, with one cause: on the densest bodies the span tree, the part it was parsed
/// from and the part it writes came to 103% of the budget, because the nodes and the attributes of a
/// tree were vectors that grow by doubling, and a vector that has stopped growing holds up to twice what
/// it uses. They closed when the tree stored them in chunks of a fixed size (`xml/arena.rs`), which hold
/// no more than they use. An entry is added with the reason the benchmark is over, and ends by itself as
/// those four did.
pub(crate) const MEMORY_GAPS: [Gap; 2] = [
    Gap {
        bench: "compare_10pct_dense",
        input: "dense-contract-300p",
        why: "compare holds two states, the alignment's texts and bags of every block and the redline's staged trees, 9% over the budget of two inputs",
    },
    Gap {
        bench: "compare_25pct_dense",
        input: "dense-contract-300p",
        why: "compare holds two states, the alignment's texts and bags of every block and the redline's staged trees, 11% over the budget of two inputs",
    },
];

/// The known gap of a benchmark on an input among `gaps`, if there is one.
pub(crate) fn memory_gap<'a>(gaps: &'a [Gap], bench: &str, input: &str) -> Option<&'a Gap> {
    gaps.iter().find(|g| g.bench == bench && g.input == input)
}

/// Whether every gap names a benchmark that exists and has a memory budget to exceed, and an input
/// that exists. A gap that names something else is a dead entry, which would exempt a benchmark
/// that took its name later. `run` checks the real gaps against the real benchmarks every time.
pub(crate) fn check_gaps(
    gaps: &[Gap],
    benches: &[Bench],
    documents: &[&str],
) -> Result<(), String> {
    for gap in gaps {
        let Some(bench) = benches.iter().find(|b| b.name == gap.bench) else {
            return Err(format!(
                "MEMORY_GAPS names the benchmark {}, which does not exist: delete or rename the entry",
                gap.bench
            ));
        };
        if !has_memory_budget(bench.limit) {
            return Err(format!(
                "MEMORY_GAPS names {} on {}, but {} has no memory budget to exceed",
                gap.bench, gap.input, gap.bench
            ));
        }
        let runs_on = match bench.inputs {
            Inputs::Documents => documents.contains(&gap.input),
            Inputs::Only(only) => only == gap.input,
        };
        if !runs_on {
            return Err(format!(
                "MEMORY_GAPS names {} on {}, and it does not run on such an input",
                gap.bench, gap.input
            ));
        }
    }
    Ok(())
}

/// Whether a benchmark of this limit is held to a memory budget.
pub(crate) fn has_memory_budget(limit: Limit) -> bool {
    matches!(limit, Limit::Op(_) | Limit::TwoInputs)
}

/// The memory budget of a benchmark on an input of `file` bytes that parses `parsed_xml` bytes of XML,
/// if it has one: the budget of an operation, and twice that for two inputs.
pub(crate) fn peak_limit(limit: Limit, file: u64, parsed_xml: u64) -> Option<u64> {
    match limit {
        Limit::Op(_) => Some(memory_budget(file, parsed_xml)),
        Limit::TwoInputs => Some(2 * memory_budget(file, parsed_xml)),
        Limit::Time(_) | Limit::None => None,
    }
}

/// The time limit of a benchmark in a mode, if it has one.
pub(crate) fn time_limit(limit: Limit, mode: Mode) -> Option<Duration> {
    let base = match limit {
        Limit::Op(op) => op_budget(op),
        Limit::Time(t) => t,
        Limit::None | Limit::TwoInputs => return None,
    };
    Some(base.mul_f64(mode.time_budget_factor()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budgets_are_those_of_the_architecture() {
        assert_eq!(op_budget(Op::Open), Duration::from_millis(300));
        assert_eq!(op_budget(Op::View), Duration::from_millis(200));
        for op in [Op::Edit, Op::Save, Op::Export] {
            assert_eq!(op_budget(op), Duration::from_secs(1));
        }
        assert_eq!(memory_budget(0, 0), 4 * MIB);
        assert_eq!(memory_budget(MIB, 2 * MIB), 4 * MIB + MIB + 16 * MIB);
        assert_eq!(
            peak_limit(Limit::TwoInputs, MIB, 2 * MIB),
            Some(2 * (4 * MIB + MIB + 16 * MIB))
        );
        assert_eq!(peak_limit(Limit::Op(Op::Open), 0, 0), Some(4 * MIB));
        assert_eq!(peak_limit(Limit::None, MIB, MIB), None);
        assert_eq!(time_limit(Limit::TwoInputs, Mode::Nightly), None);
    }

    fn bench(name: &'static str, limit: Limit, inputs: Inputs) -> Bench {
        Bench {
            name,
            limit,
            parses: super::super::spec::Parses::Main,
            inputs,
            prepare: |_| Err(String::new()),
        }
    }

    #[test]
    fn a_gap_has_to_name_a_benchmark_with_a_memory_budget_and_an_input_that_exist() {
        let benches = [
            bench("xml_serialize", Limit::Op(Op::Save), Inputs::Documents),
            bench(
                "xml_serialize_all_tags",
                Limit::Op(Op::Save),
                Inputs::Documents,
            ),
            bench("schema_part", Limit::None, Inputs::Documents),
            bench("alone", Limit::Op(Op::Edit), Inputs::Only("numbered-20k")),
            bench(
                "compare_10pct_dense",
                Limit::TwoInputs,
                Inputs::Only("dense-contract-300p"),
            ),
            bench(
                "compare_25pct_dense",
                Limit::TwoInputs,
                Inputs::Only("dense-contract-300p"),
            ),
        ];
        let inputs = ["dense-contract-300p", "node-dense-18mb"];
        assert_eq!(
            check_gaps(&MEMORY_GAPS, &benches, &inputs),
            Ok(()),
            "the gaps of today"
        );
        let gap = |bench, input| {
            [Gap {
                bench,
                input,
                why: "",
            }]
        };
        let named = |bench, input| check_gaps(&gap(bench, input), &benches, &inputs).unwrap_err();
        assert!(named("xml_serialise", "node-dense-18mb").contains("does not exist"));
        assert!(named("schema_part", "node-dense-18mb").contains("no memory budget"));
        assert!(named("alone", "node-dense-18mb").contains("does not run on such an input"));
        assert!(named("xml_serialize", "contract-300q").contains("does not run on such an input"));
        let only = [Gap {
            bench: "alone",
            input: "numbered-20k",
            why: "",
        }];
        assert_eq!(check_gaps(&only, &benches, &inputs), Ok(()));
    }

    #[test]
    fn a_ci_run_gets_twice_the_time_and_a_gate_of_its_own_counts_the_same_way() {
        let open = Limit::Op(Op::Open);
        assert_eq!(
            time_limit(open, Mode::Nightly),
            Some(Duration::from_millis(300))
        );
        assert_eq!(time_limit(open, Mode::Ci), Some(Duration::from_millis(600)));
        let gate = Limit::Time(Duration::from_millis(25));
        assert_eq!(time_limit(gate, Mode::Ci), Some(Duration::from_millis(50)));
        assert_eq!(time_limit(Limit::None, Mode::Ci), None);
    }
}
