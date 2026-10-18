//! What reading skipped or repaired. Reading is total: a tree that parsed is always read, and
//! each thing the reader had to ignore or interpret is reported here, so `check` can say what is
//! wrong with a document and tests can say what a read did. An anomaly never changes what a read
//! returns.

use crate::xml::NodeId;

/// What kind of thing a read skipped or repaired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnomalyKind {
    /// A story part (not the body) whose root is not the element its kind expects. The story is
    /// empty.
    WrongRoot,
    /// An element at table or row level that is not a row, a cell or a property. It is listed as
    /// opaque content.
    MisplacedElement,
    /// A note or comment with no usable `w:id`.
    UnitWithoutId,
    /// An element the text builder does not know where it stands. It is skipped with its content.
    UnknownElement,
    /// A paragraph or a table directly inside a paragraph.
    NestedBlock,
    /// A field character with nothing to belong to: an `end` or `separate` with no open field, a second
    /// `separate`, or instruction text outside a field.
    StrayFieldChar,
    /// A field nested deeper than 32 levels. It is ignored.
    FieldTooDeep,
    /// A field never closed before the end of its note, comment or story.
    UnterminatedField,
    /// Text in a field's code that is not instruction text. It is field code and is in no text.
    TextInFieldCode,
    /// A `w:t` inside a `w:del`, where Word writes `w:delText`. It is read as deleted text. A `w:t`
    /// in a `w:moveFrom` is not one: Word writes `w:t` there.
    TextInDeletion,
    /// A `w:delText` with no deletion around it. It is read as plain text.
    DelTextOutsideDeletion,
    /// A `w:t` with leading or trailing whitespace and no `xml:space="preserve"`. It is read as
    /// written.
    EdgeSpaceWithoutPreserve,
    /// A `w:sym` with no usable `w:char`. It reads as U+FFFD.
    InvalidSymbol,
    /// An `mc:AlternateContent` with no branch Word would read. It contributes nothing.
    NoBranch,
    /// A second or later `w:body` in a `w:document`. It is not read, and neither is anything in it.
    ExtraBody,
}

/// One thing a read skipped or repaired, and the node it is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anomaly {
    /// What happened.
    pub kind: AnomalyKind,
    /// The element it happened at.
    pub node: NodeId,
}
