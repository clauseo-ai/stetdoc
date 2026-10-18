//! The inline half of the neutral model: a paragraph's content, and the measures every file needs.
//!
//! `Inline` is what a paragraph holds, with its atoms, fields, controls and tracked changes. `width_of`
//! is an inline's width in the `Final` text, which is what `replace` offsets count, and `flatten` is
//! what a read-only seam shows of what it holds.

use crate::refs::BlockRef;
use crate::wordml::ControlKind;
use std::sync::Arc;

/// One piece of a paragraph's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Inline {
    /// Editable characters. A tab and a line break are characters of text.
    Text(String),
    /// One read-only character in the paragraph's text.
    Atom(Atom),
    /// A tracked change around inlines. A deletion inside another author's insertion is a `Rev` in a `Rev`.
    Rev {
        /// Insertion, deletion or one end of a move.
        kind: RevKind,
        /// The revision.
        tag: RevTag,
        /// What it covers.
        inlines: Vec<Inline>,
    },
    /// An external hyperlink. An internal link is transparent and has no `Link`.
    Link {
        /// Where it goes.
        target: LinkTarget,
        /// The linked text.
        inlines: Vec<Inline>,
    },
    /// A content control.
    Control {
        /// Its kind, alias and tag.
        info: ControlInfo,
        /// Whether its content can be edited.
        state: ControlState,
        /// What it holds.
        inlines: Vec<Inline>,
    },
    /// A field with its cached result, read-only.
    Field(Field),
    /// Text Word hides.
    Hidden {
        /// Why it is hidden.
        reason: HiddenReason,
        /// The hidden content.
        inlines: Vec<Inline>,
    },
    /// Text Word shows and a reader cannot see.
    LooksHidden {
        /// Why a reader cannot see it.
        signal: LooksHiddenSignal,
        /// The content.
        inlines: Vec<Inline>,
    },
    /// A zero-width mark.
    Mark(Mark),
}

/// What a tracked change around inlines is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RevKind {
    /// Inserted text.
    Ins,
    /// Deleted text.
    Del,
    /// The destination of a move.
    MoveTo,
    /// The source of a move.
    MoveFrom,
}

/// Where a link goes. Only an external target is a `Link`, so there is one variant; it is an enum for the
/// day there are two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LinkTarget {
    /// A URL.
    External(Box<str>),
}

/// What a content control is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ControlInfo {
    /// The type of control.
    pub kind: ControlKind,
    /// The control's alias, its title in Word.
    pub alias: Option<Box<str>>,
    /// The control's tag.
    pub tag: Option<Box<str>>,
}

/// Whether a content control's content can be edited.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlState {
    /// It can.
    Editable,
    /// It is locked.
    Locked,
    /// It is bound to a custom XML part, which Word refreshes it from.
    DataBound,
    /// It shows its placeholder, which is not content.
    Placeholder,
}

/// Why text is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HiddenReason {
    /// `w:vanish`.
    Vanish,
}

/// Why a reader cannot see text that Word shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LooksHiddenSignal {
    /// White text.
    White,
    /// Text of 2 points or less.
    Tiny,
}

/// A zero-width mark in a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    /// The start of a comment's range, by the comment's `w:id`.
    CommentStart(i64),
    /// The end of a comment's range.
    CommentEnd(i64),
}

/// One read-only character in a paragraph's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Atom {
    /// A page break.
    PageBreak,
    /// A column break.
    ColumnBreak,
    /// A reference to a note.
    NoteRef {
        /// Footnote or endnote.
        kind: NoteKind,
        /// The note's number as the agent sees it.
        ordinal: u32,
    },
    /// The automatic number inside a note.
    NoteMark {
        /// Footnote or endnote.
        kind: NoteKind,
    },
    /// A drawing, picture or object.
    Object(ObjectInfo),
    /// An equation, as its linear text.
    Math(Box<str>),
    /// A page number or a date that Word computes.
    Auto(AutoKind),
}

/// Footnote or endnote.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NoteKind {
    /// A footnote.
    Footnote,
    /// An endnote.
    Endnote,
}

/// What Word computes at an `Atom::Auto`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoKind {
    /// A page number.
    Page,
    /// A date.
    Date,
    /// A field with no stored result that Word still shows.
    Value,
}

/// A drawing, picture or object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObjectInfo {
    /// What it is.
    pub kind: ObjectKind,
    /// Its alt text.
    pub alt: Option<Box<str>>,
    /// The text of its text box, the paragraphs joined by ` / `.
    pub text_box: Option<Box<str>>,
}

/// What a drawing is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectKind {
    /// A picture.
    Image,
    /// A shape.
    Shape,
    /// A chart.
    Chart,
    /// Anything else embedded.
    Object,
    /// A text box.
    TextBox,
}

/// A field with its cached result. A field that spans paragraphs gives each paragraph its part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    /// The instruction's keyword, such as `REF`.
    pub keyword: Box<str>,
    /// The displayed result, with nested field codes dropped and nested atoms flattened.
    pub text: Box<str>,
    /// How many characters the result takes in the paragraph's `Final` text.
    pub width: u32,
    /// For a `REF` or `PAGEREF`, the block it resolves to.
    pub target: Option<BlockRef>,
    /// The field begins in this paragraph.
    pub opens: bool,
    /// The field ends in this paragraph.
    pub closes: bool,
    /// Word marked the result as needing an update.
    pub dirty: bool,
}

/// A tracked change as the text shows it: the id that `accept` and `reject` select by, and who made it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RevTag {
    /// The `w:id` of the revision element.
    pub id: i64,
    /// The author, an index into `Head.authors`.
    pub author: u32,
    /// The date as written.
    pub date: Option<Arc<str>>,
    /// For a move, the id of the other end.
    pub peer: Option<i64>,
}

impl Inline {
    /// The number of Unicode scalar values the inline takes in the paragraph's `Final` text:
    /// what `replace` offsets count. A deletion takes none, an atom one, a field its `width`.
    pub(crate) fn final_width(&self) -> u32 {
        match self {
            Inline::Text(text) => count(text),
            Inline::Atom(_) => 1,
            Inline::Rev {
                kind: RevKind::Del | RevKind::MoveFrom,
                ..
            } => 0,
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. }
            | Inline::Hidden { inlines, .. }
            | Inline::LooksHidden { inlines, .. } => width_of(inlines),
            Inline::Field(field) => field.width,
            Inline::Mark(_) => 0,
        }
    }
}

/// The width of a list of inlines in the `Final` text.
pub(crate) fn width_of(inlines: &[Inline]) -> u32 {
    inlines.iter().map(Inline::final_width).sum()
}

/// The number of Unicode scalar values in a string, saturating at `u32::MAX`.
pub(crate) fn count(text: &str) -> u32 {
    u32::try_from(text.chars().count()).unwrap_or(u32::MAX)
}

/// The characters of a list of inlines as one string, in `Redline` order and with every deletion
/// included: what a read-only seam shows of what it holds. An atom adds nothing and a field its result.
pub(crate) fn flatten(inlines: &[Inline]) -> String {
    let mut out = String::new();
    flatten_into(inlines, &mut out);
    out
}

fn flatten_into(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Atom(_) | Inline::Mark(_) => {}
            Inline::Field(field) => out.push_str(&field.text),
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. }
            | Inline::Hidden { inlines, .. }
            | Inline::LooksHidden { inlines, .. } => flatten_into(inlines, out),
        }
    }
}

/// The characters of a list of inlines as `Final` reads them: every deletion left out, an atom nothing and a
/// field its result.
pub(crate) fn final_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    final_into(inlines, &mut out);
    out
}

fn final_into(inlines: &[Inline], out: &mut String) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => out.push_str(text),
            Inline::Atom(_) | Inline::Mark(_) => {}
            Inline::Field(field) => out.push_str(&field.text),
            Inline::Rev {
                kind: RevKind::Del | RevKind::MoveFrom,
                ..
            } => {}
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. }
            | Inline::Hidden { inlines, .. }
            | Inline::LooksHidden { inlines, .. } => final_into(inlines, out),
        }
    }
}
