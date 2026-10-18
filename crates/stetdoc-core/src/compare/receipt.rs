//! What a comparison reports besides the redline: the moves, the warnings, what could not be redlined, and what `check` has to be
//! told about the blocks that came from the original (design `compare.md`, 2.1, 7.7 and 8.2).

use super::import::Stats;
use crate::revisions::outcome::Origin;
use crate::wordml::BlockKey;

/// A block moved: a deleted block and an inserted one whose text is nearly the same.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveKeys {
    /// The deleted block in the redline.
    pub from: BlockKey,
    /// The inserted block in the redline.
    pub to: BlockKey,
    /// The start of its text.
    pub text: String,
}

/// Something the comparison tells the reader once, with how many times it happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CompareWarning {
    /// The code, from the table of design 8.2.
    pub code: &'static str,
    /// What happened.
    pub message: String,
    /// How many.
    pub count: usize,
}

/// What the receipt names for an exclusion. Never a block of the original: its key is a node of the original's tree, which the
/// redline's refs do not read, so it would name nothing, or another paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    /// A block of the redline that is itself not written as it should be: the revised paragraph left as it is, or a copy placed.
    Block(BlockKey),
    /// The block of the redline next to which the copy of an original block would have stood, had it been made.
    Beside(BlockKey),
    /// No block of the redline: the receipt describes the original's block by its first words.
    Original,
}

/// A paragraph or block the redline does not show as it should, and why. The two compare laws leave it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Exclusion {
    /// What the receipt names.
    pub site: Site,
    /// The first words of the original's first block, when `site` names no block of the redline that is it (`Beside` and
    /// `Original`). Filled once the redline is written; empty for a block with no text.
    pub text: String,
    /// `unredlined`: the paragraph is as the revised document has it and its difference is not shown. `not_restored`: rejecting
    /// does not give the original's block back.
    pub kind: &'static str,
    /// Why it was refused.
    pub reason: String,
    /// The revised blocks (which keep their nodes in the redline) that the compare laws leave out of every dump.
    pub redline: Vec<BlockKey>,
    /// Blocks that the compare laws leave out of the rejected redline only: the copies of the original placed in it (accepting removes a
    /// deleted copy, and one that survives is the paragraph that a trailing deletion joined to a live one, which is the revised
    /// paragraph), and the inserted paragraphs before an excluded one, whose rejection joins them to it.
    pub copies: Vec<BlockKey>,
    /// The blocks of the original that the compare laws leave out of the original's dump.
    pub original: Vec<BlockKey>,
}

/// What `check` needs: where each new block came from, and which blocks of the redline hold content of the original.
#[derive(Clone, Debug, Default)]
pub(crate) struct Declared {
    /// The origins that the calls declared.
    pub origins: Vec<Origin>,
    /// Each block of the redline that received content from the original, and the original's block.
    pub imports: Vec<(BlockKey, BlockKey)>,
    /// Every block that a call wrote to: the blocks that the plan does not keep (N4 reads the rest as untouched).
    pub touched: Vec<BlockKey>,
    /// The style and numbering definitions and the parts that the import added, each with what it was copied from: foreign
    /// origins, which `check` keys by the original's element or part.
    pub definitions: Vec<crate::revisions::redline_types::ImportedDef>,
}

/// How much of each kind of thing the plan held.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    /// Pairs equal in text and formatting.
    pub kept: usize,
    /// Paragraphs with edits.
    pub edited: usize,
    /// Blocks marked inserted, and placed as deleted.
    pub inserted: usize,
    /// As above.
    pub deleted: usize,
    /// Splits and merges written.
    pub split: usize,
    /// As above.
    pub merged: usize,
}

/// The receipt of a comparison.
#[derive(Clone, Debug, Default)]
pub(crate) struct CompareReceipt {
    /// The moves.
    pub moves: Vec<MoveKeys>,
    /// The warnings, each once.
    pub warnings: Vec<CompareWarning>,
    /// What could not be redlined.
    pub exclusions: Vec<Exclusion>,
    /// What the plan held.
    pub counts: Counts,
    /// What the import left out or added.
    pub stats: Stats,
    /// What `check` is told.
    pub declared: Declared,
    /// Calls that failed after they had written something: a defect, and it is a count so that no comparison hides one.
    pub broken: usize,
    /// The revisions accepted in the original and in the revised document.
    pub accepted: (usize, usize),
}
