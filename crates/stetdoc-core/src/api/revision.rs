//! A revision, as `Document::revisions` lists it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a change is. A **change** is the unit that an agent thinks in: a move with both halves, a paragraph deletion with
/// its mark, an Enter pair, a split insertion with another author's change between its pieces.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ChangeKind {
    /// Inserted text.
    Insertion,
    /// Deleted text.
    Deletion,
    /// Moved text, both halves.
    Move,
    /// A whole inserted paragraph.
    ParagraphInserted,
    /// A whole deleted paragraph.
    ParagraphDeleted,
    /// An inserted paragraph mark only: a split.
    MarkInserted,
    /// A deleted paragraph mark only: a merge.
    MarkDeleted,
    /// An inserted table row.
    RowInserted,
    /// A deleted table row.
    RowDeleted,
    /// An inserted table cell.
    CellInserted,
    /// A deleted table cell.
    CellDeleted,
    /// Merged table cells.
    CellMerged,
    /// A change to run formatting.
    RunFormat,
    /// A change to paragraph formatting.
    ParagraphFormat,
    /// A change to section properties.
    SectionFormat,
    /// A change to table properties.
    TableFormat,
    /// A change to row properties.
    RowFormat,
    /// A change to cell properties.
    CellFormat,
    /// A change to the table grid.
    GridChange,
    /// A change to a numbering label.
    NumberingChange,
}

/// One change in the document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RevisionInfo {
    /// Every revision id of the change, in document order. Accepting or rejecting any of them resolves the whole change.
    pub ids: Vec<i64>,
    /// What the change is.
    pub kind: ChangeKind,
    /// Who made it, as the document writes the author, if it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// When, as the document writes the date, if it says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// The ref of the first block the change touches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// The story: `body`, `notes`, `headers` or `comments`.
    pub story: String,
    /// The text the change inserts, cut at 200 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inserted: Option<String>,
    /// The text the change deletes, cut at 200 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<String>,
    /// True when `inserted` or `deleted` was cut.
    #[serde(default)]
    pub text_truncated: bool,
}
