//! The vocabulary of a draft: every read-only fact of a line, in no syntax.
//!
//! A `Key` is a closed enum and not a string, so that two keys are equal exactly when they say the
//! same thing, and so that a format cannot invent a spelling that happens to match. Every part of
//! what a format shows of a read-only thing is in its key (the target of a field, the lines of a
//! text box, the alt text of a picture, the id and author of a change), so that an edit of any of it
//! is a change of the key and is refused. `describe` gives a key a name for a message, in plain
//! words, because a message must not depend on a format's syntax.

use super::model::{
    AutoKind, CellChange, ChangeKind, ControlState, LooksHiddenSignal, Margin, MarkKind, NoteKind,
    ObjectKind, OpaqueBlock, RowChange, SectionUse,
};
use crate::refs::BlockRef;
use crate::wordml::{ControlKind, SectionBreak};

/// A tracked change as an agent addresses it: the id that `accept` and `reject` select by and the
/// code of the author. For a move, the id of the other end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RevRef {
    /// The `w:id` of the revision element.
    pub id: i64,
    /// The author's code from the head.
    pub author: Box<str>,
    /// For a move, the id of the other end.
    pub peer: Option<i64>,
}

/// What an `Open` and a `Close` delimit: editable text inside a read-only wrapper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Span {
    /// Inserted text.
    Ins(RevRef),
    /// The destination of a move.
    MoveIn(RevRef),
    /// An external link, by its URL.
    Link(Box<str>),
    /// An editable content control.
    Control(ControlKey),
}

/// The kind, alias and tag of a content control.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ControlKey {
    /// The type of control.
    pub kind: ControlKind,
    /// Its alias.
    pub alias: Option<Box<str>>,
    /// Its tag.
    pub tag: Option<Box<str>>,
}

/// The start or the end of a comment's range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Edge {
    /// The start.
    Start,
    /// The end.
    End,
}

/// A list label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LabelKey {
    /// The number, or for a bullet its glyph.
    pub text: Box<str>,
    /// True for a bullet.
    pub bullet: bool,
    /// The label of a deleted paragraph.
    pub ghost: bool,
    /// The level hides the label.
    pub hidden: bool,
}

/// A drawing, picture or object, with everything a format shows of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObjectKey {
    /// What it is.
    pub kind: ObjectKind,
    /// Its alt text.
    pub alt: Option<Box<str>>,
    /// The text of its text box.
    pub text_box: Option<Box<str>>,
}

/// A field with its result and target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldKey {
    /// The instruction's keyword.
    pub keyword: Box<str>,
    /// The displayed result.
    pub text: Box<str>,
    /// The block a `REF` or `PAGEREF` resolves to.
    pub target: Option<BlockRef>,
    /// The field begins in this paragraph.
    pub opens: bool,
    /// The field ends in this paragraph.
    pub closes: bool,
    /// The result needs an update.
    pub dirty: bool,
}

/// A change anchored at a block that is not text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropKey {
    /// What changed.
    pub kind: ChangeKind,
    /// The revision.
    pub rev: RevRef,
}

/// A table line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableKey {
    /// The table's ref.
    pub r#ref: Option<BlockRef>,
    /// The length of the grid.
    pub cols: u32,
    /// The style's name.
    pub style: Option<Box<str>>,
    /// Property changes of the table.
    pub revs: Vec<PropKey>,
    /// The cell tags, when the table is a block of a cell.
    pub cell: Vec<Key>,
}

/// A row line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowKey {
    /// The row's ref.
    pub r#ref: Option<BlockRef>,
    /// A header row.
    pub header: bool,
    /// The row was inserted or deleted.
    pub change: Option<(RowChange, RevRef)>,
    /// Property changes of the row.
    pub revs: Vec<PropKey>,
}

/// A section line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SectionKey {
    /// The section's number.
    pub index: u32,
    /// How the next section starts.
    pub break_type: SectionBreak,
    /// The next section has a different first page.
    pub title_pg: bool,
    /// The mark that carries the break was inserted or deleted.
    pub mark: Option<(MarkKind, RevRef)>,
    /// Property changes of the section.
    pub revs: Vec<PropKey>,
}

/// The line that opens a unit other than the body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UnitKey {
    /// A footnote or an endnote, by its number.
    Note {
        /// True for an endnote.
        endnote: bool,
        /// The note's number.
        ordinal: u32,
        /// The number Word shows as its mark in this view, when it is not `ordinal`.
        mark: Option<u32>,
    },
    /// A header or footer part and the sections that use it.
    Header {
        /// True for a footer.
        footer: bool,
        /// The sections and the slots.
        uses: Vec<SectionUse>,
    },
    /// A comment or a reply.
    Comment {
        /// The comment's `w:id`.
        id: i64,
        /// The author's code.
        author: Option<Box<str>>,
        /// The date as written.
        date: Option<Box<str>>,
        /// The block it is anchored on.
        on: Option<BlockRef>,
        /// The thread is resolved.
        resolved: bool,
        /// The comment it replies to.
        reply_to: Option<i64>,
    },
}

/// One read-only fact of a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Key {
    // Tags of a paragraph.
    /// The style's name.
    Style(Box<str>),
    /// The list label.
    Label(LabelKey),
    /// The paragraph mark was inserted, deleted or moved.
    Mark {
        /// What happened to it.
        kind: MarkKind,
        /// The revision.
        rev: RevRef,
    },
    /// A property change anchored at the paragraph.
    Rev(PropKey),
    /// Word hides the paragraph.
    HiddenPara,
    /// The paragraph continues in the next one.
    JoinsNext,
    /// Another block of the cell that the previous line began.
    CellPlus,
    /// The cell spans this many columns.
    CellSpan(u32),
    /// The cell's place in a vertical merge.
    CellVMerge {
        /// True for the cell that starts the region.
        restart: bool,
    },
    /// The cell was inserted, deleted or merged.
    CellChange {
        /// What happened.
        kind: CellChange,
        /// The revision.
        rev: RevRef,
    },
    // Seams inside a paragraph.
    /// The start of editable text inside a wrapper.
    Open(Span),
    /// The end of editable text inside a wrapper.
    Close(Span),
    /// Deleted text, or the source of a move.
    Deleted {
        /// True for the source of a move.
        moved: bool,
        /// The revision.
        rev: RevRef,
        /// The text.
        text: Box<str>,
    },
    /// A field and its result.
    Field(FieldKey),
    /// Hidden text.
    Hidden(Box<str>),
    /// Text that Word shows and a reader cannot see.
    LooksHidden {
        /// Why a reader cannot see it.
        signal: LooksHiddenSignal,
        /// The text.
        text: Box<str>,
    },
    /// A content control whose content cannot be edited.
    Control {
        /// Its kind, alias and tag.
        info: ControlKey,
        /// Why it cannot be edited.
        state: ControlState,
        /// What it holds.
        text: Box<str>,
    },
    /// The start or end of a comment's range.
    Comment {
        /// The comment's `w:id`.
        id: i64,
        /// Which end.
        edge: Edge,
    },
    /// A reference to a note.
    NoteRef {
        /// Footnote or endnote.
        kind: NoteKind,
        /// The note's number.
        ordinal: u32,
    },
    /// The automatic number in a note.
    NoteMark(NoteKind),
    /// A drawing, picture or object.
    Object(ObjectKey),
    /// An equation, as its linear text.
    Math(Box<str>),
    /// A page break.
    PageBreak,
    /// A column break.
    ColumnBreak,
    /// A page number or a date that Word computes.
    Auto(AutoKind),
    // Structure lines.
    /// A table line.
    Table(TableKey),
    /// A row line.
    Row(RowKey),
    /// The end of a table.
    EndTable,
    /// A section break.
    Section(SectionKey),
    /// Content the model cannot show, or a collapsed field result.
    Opaque(OpaqueBlock),
    /// The line that opens a note, a header or footer, or a comment.
    Unit(UnitKey),
    /// The text a comment is anchored to.
    Anchored(Box<str>),
    /// A line in the margin of a page. Only a view has them.
    Margin(Margin),
}

impl RevRef {
    fn show(&self) -> String {
        format!("#{} by {}", self.id, self.author)
    }
}

/// A few characters of a text for a message, with the text cut short and quoted.
fn quoted(text: &str) -> String {
    const SHOWN: usize = 40;
    let mut out: String = text.chars().take(SHOWN).collect();
    if text.chars().nth(SHOWN).is_some() {
        out.push_str("...");
    }
    format!("{out:?}")
}

impl Key {
    /// The key named in plain words, for a message: "the deletion #4 by JD of \"Losses\"".
    pub(crate) fn describe(&self) -> String {
        match self {
            Key::Style(name) => format!("the style {}", quoted(name)),
            Key::Label(label) if label.bullet => "the bullet".to_string(),
            Key::Label(label) => format!("the number {}", quoted(&label.text)),
            Key::Mark { kind, rev } => {
                format!("the paragraph mark change ({kind:?}, {})", rev.show())
            }
            Key::Rev(prop) => format!("the {:?} change {}", prop.kind, prop.rev.show()),
            Key::HiddenPara => "the hidden-paragraph flag".to_string(),
            Key::JoinsNext => "the joins-next flag".to_string(),
            Key::CellPlus | Key::CellSpan(_) | Key::CellVMerge { .. } | Key::CellChange { .. } => {
                "the cell structure".to_string()
            }
            Key::Open(span) | Key::Close(span) => match span {
                Span::Ins(rev) | Span::MoveIn(rev) => {
                    format!("the tracked insertion {}", rev.show())
                }
                Span::Link(url) => format!("the link to {}", quoted(url)),
                Span::Control(_) => "the content control".to_string(),
            },
            Key::Deleted { moved, rev, text } => format!(
                "the {} {} of {}",
                if *moved { "move away" } else { "deletion" },
                rev.show(),
                quoted(text)
            ),
            Key::Field(field) => format!("the {} field {}", field.keyword, quoted(&field.text)),
            Key::Hidden(text) => format!("the hidden text {}", quoted(text)),
            Key::LooksHidden { text, .. } => {
                format!("the text a reader cannot see {}", quoted(text))
            }
            Key::Control { info, text, .. } => {
                format!("the read-only {:?} control {}", info.kind, quoted(text))
            }
            Key::Comment { id, .. } => format!("the mark of comment c{id}"),
            Key::NoteRef { kind, ordinal } => format!("the reference to {kind:?} {ordinal}"),
            Key::NoteMark(kind) => format!("the {kind:?} number"),
            Key::Object(object) => format!("the {:?} object", object.kind),
            Key::Math(text) => format!("the equation {}", quoted(text)),
            Key::PageBreak => "the page break".to_string(),
            Key::ColumnBreak => "the column break".to_string(),
            Key::Auto(kind) => format!("the computed {kind:?}"),
            Key::Table(table) => match table.r#ref {
                Some(table_ref) => format!("the table line {table_ref}"),
                None => "the table line".to_string(),
            },
            Key::Row(row) => match row.r#ref {
                Some(row_ref) => format!("the row line {row_ref}"),
                None => "the row line".to_string(),
            },
            Key::EndTable => "the end of the table".to_string(),
            Key::Section(section) => format!("the end of section {}", section.index),
            Key::Opaque(_) => "the block that is not shown".to_string(),
            Key::Unit(_) => "the line that opens this unit".to_string(),
            Key::Anchored(text) => format!("the anchored text {}", quoted(text)),
            Key::Margin(_) => "a margin note".to_string(),
        }
    }
}
