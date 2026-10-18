//! The plain data that the compiler and the resolver take (design `revisions.md`, 2.1 and 6.1).

use crate::wordml::BlockKey;
use std::ops::Range;
use std::sync::Arc;

/// Before or after the block that a new block is placed against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// The new block goes before the anchor.
    Before,
    /// The new block goes after the anchor.
    After,
}

/// A run of insertions in which each paragraph goes after the one the insertion before it made, as `ops::insert`
/// makes the paragraphs of one operation. A story is scanned again after every edit, so looking the neighbours of
/// every new paragraph up in it made a run cost the square of its length. Every paragraph of a run is followed by
/// the same block of its container, so the run remembers it, and only the first insertions look it up.
#[derive(Clone, Debug, Default)]
pub(crate) struct InsertRun {
    /// What the last insertion of the run made, if there was one.
    pub(super) last: Option<RunStep>,
}

/// What an insertion of a run made.
#[derive(Clone, Debug)]
pub(crate) struct RunStep {
    /// The paragraph it made.
    pub(super) made: BlockKey,
    /// The block of its container that follows it in the same parent, if one does.
    pub(super) follows: Option<(crate::wordml::NodeId, crate::wordml::BlockKind)>,
    /// True if no field is open after it: its anchor was a paragraph whose field depth was checked.
    pub(super) settled: bool,
}

/// What to do with the revisions that are selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Make the change.
    Accept,
    /// Undo the change.
    Reject,
}

/// Which revisions a step selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selector {
    /// Every revision in every part.
    All,
    /// Every revision with this `w:author` (an empty string selects a missing or empty author).
    Author(String),
    /// The change that contains each of these ids.
    Ids(Vec<i64>),
    /// Exactly the elements with these ids, not grouped into changes and with no closure. The laws use
    /// it, so it exists in test builds only.
    #[cfg(test)]
    Elements(Vec<i64>),
}

/// One accept or reject. `ops` hands the resolver the steps of consecutive operations together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    /// What to do.
    pub action: Action,
    /// What to do it to.
    pub selector: Selector,
}

/// A new element described by names: a namespace URI, a local name, attributes and children. It is
/// the one description of a new element in this module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropElement {
    /// The namespace URI, or empty for none.
    pub ns: String,
    /// The local name.
    pub local: String,
    /// The attributes as (namespace URI or empty, local name, value).
    pub attrs: Vec<(String, String, String)>,
    /// The children, in order.
    pub children: Vec<PropElement>,
}

/// What a property edit changes: elements to set, and names to clear, each as (namespace URI, local
/// name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PropEdit {
    /// Elements that replace or add the child of the same name.
    pub set: Vec<PropElement>,
    /// Names of children to remove.
    pub clear: Vec<(String, String)>,
}

impl PropEdit {
    /// Whether the edit changes nothing.
    pub(crate) fn is_empty(&self) -> bool {
        self.set.is_empty() && self.clear.is_empty()
    }
}

/// A new paragraph: its text (a tab is `\t`, a line break `\n`) and the properties that differ from the
/// paragraph it is modelled on. No properties means "the same as that paragraph".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct NewParagraph {
    /// The text of the paragraph.
    pub text: String,
    /// What differs from the paragraph it is modelled on.
    pub props: PropEdit,
    /// The `w:styleId` of the paragraph. With `Some`, the new paragraph's base properties are exactly
    /// that `w:pStyle` and nothing copied from the model paragraph; with `None`, the model's.
    pub style: Option<String>,
}

/// The container of a property edit. `Run` covers the direct formatting of the runs that hold the range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PropTarget {
    /// The `w:rPr` of every run that holds the range, a range of the paragraph's `Final` text.
    Run {
        /// The paragraph.
        block: BlockKey,
        /// The characters.
        range: Range<usize>,
    },
    /// The paragraph mark's `w:pPr/w:rPr`.
    ParagraphMark {
        /// The paragraph.
        block: BlockKey,
    },
    /// The paragraph's `w:pPr`.
    Paragraph {
        /// The paragraph.
        block: BlockKey,
    },
    /// The `w:sectPr` of the paragraph that ends a section.
    Section {
        /// The paragraph whose `w:pPr` holds the `w:sectPr`.
        block: BlockKey,
    },
    /// The `w:sectPr` that ends the body of a part.
    BodySection {
        /// The part.
        part: Arc<str>,
    },
    /// A table's `w:tblPr`.
    Table {
        /// The table.
        table: BlockKey,
    },
    /// A row's `w:trPr`.
    Row {
        /// The row.
        row: BlockKey,
    },
    /// The `w:tcPr` of the cell at an index of a row.
    Cell {
        /// The row.
        row: BlockKey,
        /// The index among the cells of the row.
        cell: usize,
    },
}

/// Range markers for `place_markers`: the start element, the end element and, when the caller wants
/// one, the run that holds the reference (a comment's `w:commentReference`). The caller mints the ids.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkerSpec {
    /// The element at the start of the range.
    pub start: PropElement,
    /// The element at the end of the range.
    pub end: PropElement,
    /// The run that holds the reference, placed right after the end marker.
    pub reference: Option<PropElement>,
}

/// What a change is, for `changes()` and the receipts (6.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// Inserted text.
    Insertion,
    /// Deleted text.
    Deletion,
    /// Text moved, both halves.
    Move,
    /// A paragraph inserted with its mark.
    ParagraphInserted,
    /// A paragraph deleted with its mark.
    ParagraphDeleted,
    /// An inserted paragraph mark with plain content: a split.
    MarkInserted,
    /// A deleted paragraph mark with plain content: a merge.
    MarkDeleted,
    /// A row inserted.
    RowInserted,
    /// A row deleted.
    RowDeleted,
    /// A cell inserted.
    CellInserted,
    /// A cell deleted.
    CellDeleted,
    /// Cells merged.
    CellMerged,
    /// Run formatting changed.
    RunFormat,
    /// Paragraph properties changed.
    ParagraphFormat,
    /// Section properties changed.
    SectionFormat,
    /// Table properties changed.
    TableFormat,
    /// Row properties changed.
    RowFormat,
    /// Cell properties changed.
    CellFormat,
    /// The table grid changed.
    GridChange,
    /// A numbering label changed.
    NumberingChange,
}
