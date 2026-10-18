//! What a benchmark is: a name, what it is held to, and how to prepare one run of it.

use std::any::Any;
use std::time::Duration;

/// The operations that have a speed or memory budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// Open a document: the block scan, with the parts parsed lazily.
    Open,
    /// Render the outline or one page of text.
    View,
    /// Fifty operations, with the validation of the dirty parts.
    Edit,
    /// Write the package.
    Save,
    /// Write the working copy.
    Export,
}

/// What a benchmark is held to, besides its regression against the baseline.
#[derive(Clone, Copy, Debug)]
pub enum Limit {
    /// The budget of an operation that the benchmark is part of, or all of. A part may not take
    /// longer than the whole.
    Op(Op),
    /// A gate of its own, such as the 25 ms of the style cascade.
    Time(Duration),
    /// Nothing but the baseline: the work has no budget of its own.
    None,
    /// Two documents open at once, as compare holds them: no time budget of its own, and the memory budget
    /// of two inputs, the sum of the memory budgets of the original and the revised (`docs/design/compare.md`,
    /// section 10). The revised document is made from the input with a share of its blocks changed, so each
    /// side is counted at the size of the input.
    TwoInputs,
}

/// The XML parts that a run inflates and parses, which the memory budget is relative to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parses {
    /// None: the run works on the archive alone.
    Nothing,
    /// The main part.
    Main,
    /// Every XML part.
    All,
}

/// The inputs a benchmark runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inputs {
    /// Every document input of `inputs::NAMES`, one run each.
    Documents,
    /// One input that only the benchmarks that name it run on, such as `inputs::NUMBERED`, the
    /// body that the gates of `numbering` are written for. It runs once, and the memory budget is
    /// that of this input.
    Only(&'static str),
}

/// What a run returns: whatever the operation made and what it was made from. The harness drops
/// it after the clock stops, so that freeing a tree is not part of the time of building it.
pub type Kept = Box<dyn Any>;

/// One run of an operation, ready to go. Calling it is what is timed.
pub type Run = Box<dyn FnOnce() -> Result<Kept, String>>;

/// One benchmark. It is a constant of a small file in `stetdoc-core/src/bench/`, listed in `all()`
/// of that module, because what it measures is private to the core.
#[derive(Clone, Copy)]
pub struct Bench {
    /// A stable name: `package_open`. The baseline is keyed by it.
    pub name: &'static str,
    /// What it is held to.
    pub limit: Limit,
    /// The XML it parses, for the memory budget.
    pub parses: Parses,
    /// The inputs it runs on.
    pub inputs: Inputs,
    /// Prepares one run on an input. What this builds and the run keeps counts toward the peak
    /// memory, as a caller that holds a document does. Everything else is not timed.
    pub prepare: fn(&Input) -> Result<Run, String>,
}

/// One document to run on, as plain data.
#[derive(Clone, Debug)]
pub struct Input {
    /// A stable name: `contract-300p`.
    pub name: &'static str,
    /// The `.docx` file.
    pub docx: Vec<u8>,
    /// The main part, inflated, so that a benchmark of the XML does not need the archive.
    pub main_part: Vec<u8>,
    /// The sum of the inflated sizes of every XML part.
    pub xml_bytes: u64,
}

impl Input {
    /// How many bytes of XML a run that `parses` inflates and parses.
    pub(crate) fn parsed(&self, parses: Parses) -> u64 {
        match parses {
            Parses::Nothing => 0,
            Parses::Main => self.main_part.len() as u64,
            Parses::All => self.xml_bytes,
        }
    }

    /// An input with nothing in it, for a test of the harness that has no document to give.
    pub fn none() -> Input {
        Input {
            name: "-",
            docx: Vec::new(),
            main_part: Vec::new(),
            xml_bytes: 0,
        }
    }
}
