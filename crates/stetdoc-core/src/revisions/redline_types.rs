//! The input types of the compare primitives (design `compare.md`, 6.1): what `redline_paragraph`, `mark_inserted`,
//! `place_deleted` and `set_snapshot` are given. The original's content arrives as **detached nodes in the transaction's own
//! tree**, already copied and re-pointed by `compare::import`, so these primitives never read another package.

use super::types::PropElement;
use crate::wordml::BlockKey;
use crate::wordml::NodeId;
use std::ops::Range;
use std::sync::Arc;

/// One replacement in a paragraph of the revised document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RedlineEdit {
    /// The characters of the paragraph's `Final` text that are inserted: wrapped in `w:ins`. Empty for a pure deletion,
    /// where it is the place the deleted text goes.
    pub insert: Range<usize>,
    /// The deleted text, as detached nodes of the transaction's tree: runs, or wrappers (a hyperlink) that hold runs. They
    /// are placed in `w:del` immediately before the insertion.
    pub deleted: Vec<NodeId>,
}

/// The run properties that a stretch of unchanged text had in the original: an `rPrChange` per run with this snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunSnapshot {
    /// The characters of the revised paragraph.
    pub range: Range<usize>,
    /// The children of the old `w:rPr`.
    pub snapshot: Vec<PropElement>,
}

/// The old properties of a paragraph, for its `pPrChange`, and of its mark, for the `rPrChange` in the mark.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// The children of the old `w:pPr` (without `w:rPr`, `w:sectPr` and `w:pPrChange`), if the paragraph's properties differ.
    pub ppr: Option<Vec<PropElement>>,
    /// The children of the old mark `w:rPr`, if the mark's properties differ.
    pub mark: Option<Vec<PropElement>>,
}

/// What `redline_paragraph` writes into one paragraph.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParaRedline {
    /// The replacements.
    pub edits: Vec<RedlineEdit>,
    /// The formatting that changed over unchanged text.
    pub fmt: Vec<RunSnapshot>,
    /// The old properties of the paragraph and its mark.
    pub snapshot: Option<Snapshot>,
    /// The mark alone is inserted: `w:ins` in the mark's `w:rPr`, nothing on the content.
    pub mark_inserted: bool,
    /// The mark is deleted: the last live paragraph before a run of trailing deletions.
    pub mark_deleted: bool,
    /// The original paragraph that the fragments come from, for the foreign origin.
    pub source: Option<BlockKey>,
}

/// A detached block of the transaction's tree that came from another package, and the block it was copied from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Imported {
    /// The detached paragraph, row or table.
    pub node: NodeId,
    /// The block in the original package.
    pub source: BlockKey,
}

/// A definition or a part that `compare`'s import added to the transaction, and what it was copied from in the other package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImportedDef {
    /// The part of the transaction it is in, or the part that was added.
    pub part: Arc<str>,
    /// The element (a `w:style`, a `w:abstractNum`, a `w:num`), or `None` for a whole part.
    pub node: Option<NodeId>,
    /// The part of the other package it was copied from.
    pub source_part: Arc<str>,
    /// The element there, or `None` for a whole part.
    pub source_node: Option<NodeId>,
}

impl ParaRedline {
    /// The same plan with every range moved `by` characters later: the plan of a part of a paragraph, read against the whole.
    pub(crate) fn shifted(&self, by: usize) -> ParaRedline {
        ParaRedline {
            edits: self
                .edits
                .iter()
                .map(|e| RedlineEdit {
                    insert: e.insert.start + by..e.insert.end + by,
                    deleted: e.deleted.clone(),
                })
                .collect(),
            fmt: self
                .fmt
                .iter()
                .map(|f| RunSnapshot {
                    range: f.range.start + by..f.range.end + by,
                    snapshot: f.snapshot.clone(),
                })
                .collect(),
            ..self.clone()
        }
    }
}
