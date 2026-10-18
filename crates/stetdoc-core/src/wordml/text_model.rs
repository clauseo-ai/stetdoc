//! The text model: what a paragraph's content becomes, and the map from characters to XML.
//!
//! **Start here.** `Paragraph::map` builds a [`TextMap`]: one list of [`Item`]s in XML document
//! order, from which the three [`Mode`]s are masks. Every item keeps its XML node, its run, the
//! chain of wrappers around it (a revision, a hyperlink, a content control), its field and which of
//! `Final` and `Original` contain it. The map is the same structure: `locate`, `gap` and `offset_of`
//! turn character offsets into XML positions and back (`text_locate.rs`).
//!
//! The design is `docs/design/wordml.md`, sections 4 and 5, and the invariants of 5.2 are what the
//! tests check:
//!
//! 1. Items are in XML document order, and `(kind, node)` is unique.
//! 2. The Redline text is the concatenation of every item that is not field code.
//! 3. A `Text` item is characters `[from, from + len)` of its node, and every other item is the one
//!    character of its kind (or none, for a mark).
//! 4. A mode's text is the concatenation of the items the mode includes. With no revision wrapper
//!    the three texts are equal.
//! 5. Field code is in no mode's text, however many fields are nested in it.
//! 6. An item is in `Final` unless a deletion wrapper encloses it, and in `Original` unless an
//!    insertion wrapper does, whatever the element names, and the wrappers include those above the
//!    paragraph up to the story root.
//!
//! The files: `text_build.rs` and `text_content.rs` build a map, `text_fields.rs` is the field
//! machine and the field index, `text_locate.rs` the operations. Offsets are counts of Unicode
//! scalar values everywhere.

use super::anomaly::Anomaly;
use super::text_fields::OpenField;
use crate::api::view::Mode;
use crate::xml::NodeId;
use std::borrow::Cow;

/// What an item is. Marks have no width: they sit between items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    /// Characters of a text or CDATA node of a `w:t` or `w:delText`.
    Text,
    /// A tab, `w:tab` or `w:ptab`: U+0009.
    Tab,
    /// A line break, `w:br` or `w:cr`: U+000A.
    LineBreak,
    /// A page break, `w:br w:type="page"`: U+000C.
    PageBreak,
    /// A column break, `w:br w:type="column"`: U+000E.
    ColumnBreak,
    /// A non-breaking hyphen, `w:noBreakHyphen`: U+2011.
    Hyphen,
    /// An optional hyphen, `w:softHyphen`: U+00AD.
    SoftHyphen,
    /// A symbol, `w:sym`: the code point of its `w:char`.
    Symbol,
    /// A drawing, picture or object: U+0001.
    Object,
    /// A footnote or endnote reference: U+0002.
    NoteRef,
    /// The automatic number inside a note: U+0003.
    NoteMark,
    /// An equation: U+0004.
    Math,
    /// A page number or a legacy date element: U+0005.
    Auto,
    /// A field with no stored result that Word still shows (4.4): U+0006.
    FieldValue,
    /// `w:bookmarkStart`.
    BookmarkStart,
    /// `w:bookmarkEnd`.
    BookmarkEnd,
    /// `w:commentRangeStart`.
    CommentStart,
    /// `w:commentRangeEnd`.
    CommentEnd,
    /// The `w:commentReference` in a run. It has a position and no width.
    CommentRef,
    /// The start of a field: a `w:fldChar` `begin`, or a `w:fldSimple`.
    FieldBegin,
    /// A `w:fldChar` `separate`.
    FieldSeparate,
    /// The end of a field: a `w:fldChar` `end`, or the end of a `w:fldSimple`.
    FieldEnd,
    /// Field code: a `w:instrText`, `w:delInstrText` or other content in the code phase.
    FieldCode,
    /// `w:permStart`.
    PermStart,
    /// `w:permEnd`.
    PermEnd,
    /// `w:moveFromRangeStart` or `w:moveToRangeStart`.
    MoveRangeStart,
    /// `w:moveFromRangeEnd` or `w:moveToRangeEnd`.
    MoveRangeEnd,
}

impl ItemKind {
    /// True for an item with no width.
    pub fn is_mark(self) -> bool {
        use ItemKind::{
            BookmarkEnd, BookmarkStart, CommentEnd, CommentRef, CommentStart, FieldBegin,
            FieldCode, FieldEnd, FieldSeparate, MoveRangeEnd, MoveRangeStart, PermEnd, PermStart,
        };
        matches!(
            self,
            BookmarkStart
                | BookmarkEnd
                | CommentStart
                | CommentEnd
                | CommentRef
                | FieldBegin
                | FieldSeparate
                | FieldEnd
                | FieldCode
                | PermStart
                | PermEnd
                | MoveRangeStart
                | MoveRangeEnd
        )
    }

    /// The one character of an atom. `None` for text, a symbol (its character is its own) and a
    /// mark.
    pub fn sentinel(self) -> Option<char> {
        match self {
            ItemKind::Tab => Some('\u{9}'),
            ItemKind::LineBreak => Some('\u{a}'),
            ItemKind::PageBreak => Some('\u{c}'),
            ItemKind::ColumnBreak => Some('\u{e}'),
            ItemKind::Hyphen => Some('\u{2011}'),
            ItemKind::SoftHyphen => Some('\u{ad}'),
            ItemKind::Object => Some('\u{1}'),
            ItemKind::NoteRef => Some('\u{2}'),
            ItemKind::NoteMark => Some('\u{3}'),
            ItemKind::Math => Some('\u{4}'),
            ItemKind::Auto => Some('\u{5}'),
            ItemKind::FieldValue => Some('\u{6}'),
            _ => None,
        }
    }
}

/// Which modes include an item: the `FINAL` and `ORIGINAL` bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vis(pub u8);

impl Vis {
    /// In `Final`: no deletion wrapper encloses the item.
    pub const FINAL: u8 = 1;
    /// In `Original`: no insertion wrapper encloses the item.
    pub const ORIGINAL: u8 = 2;
    /// In both: no revision wrapper encloses the item.
    pub const BOTH: Vis = Vis(Vis::FINAL | Vis::ORIGINAL);

    /// True if the mode includes the item. `Redline` includes every item.
    pub fn in_mode(self, mode: Mode) -> bool {
        match mode {
            Mode::Redline => true,
            Mode::Final => self.0 & Vis::FINAL != 0,
            Mode::Original => self.0 & Vis::ORIGINAL != 0,
        }
    }
}

/// An item that is not in a field.
pub const NO_FIELD: u32 = u32::MAX;
/// The item is in a field's result: the caller decides whether that is read-only.
pub const FIELD_RESULT: u8 = 1;
/// The item's text node is a CDATA section, which the editor cannot edit in place.
pub const CDATA: u8 = 2;

/// One piece of a paragraph's content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Item {
    /// What it is.
    pub kind: ItemKind,
    /// The text node of a `Text` item, else the element.
    pub node: NodeId,
    /// The nearest `w:r`, for formatting and hidden-text rules.
    pub run: Option<NodeId>,
    /// The character offset in the Redline text where the item starts.
    pub at: u32,
    /// Characters. Zero for a mark.
    pub len: u32,
    /// For `Text`: where in the node's decoded characters this item starts.
    pub from: u32,
    /// The innermost wrapper: an index into the map's contexts, or [`NO_CONTEXT`].
    pub ctx: u32,
    /// Which modes include it.
    pub vis: Vis,
    /// The id of the innermost field it is in, or [`NO_FIELD`]. A field id is an internal handle, the
    /// ordinal of the field's start in its story: adding or removing a field renumbers every later
    /// one, so it is valid for this map and no longer, and must not be kept across an edit.
    pub field: u32,
    /// `FIELD_RESULT` and `CDATA`.
    pub flags: u8,
    /// Where its characters start in the Redline text, in bytes.
    pub(super) byte: u32,
    /// How many bytes its characters take.
    pub(super) blen: u32,
}

/// An item with no wrapper around it.
pub const NO_CONTEXT: u32 = u32::MAX;

/// What kind of element a context is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WrapperKind {
    /// `w:ins`.
    Ins,
    /// `w:del`.
    Del,
    /// `w:moveFrom`.
    MoveFrom,
    /// `w:moveTo`.
    MoveTo,
    /// A row inserted by a tracked change: `w:trPr/w:ins`.
    RowIns,
    /// A row deleted by a tracked change: `w:trPr/w:del`.
    RowDel,
    /// A cell inserted by a tracked change: `w:tcPr/w:cellIns`.
    CellIns,
    /// A cell deleted by a tracked change: `w:tcPr/w:cellDel`.
    CellDel,
    /// `w:hyperlink`.
    Hyperlink,
    /// `w:fldSimple`.
    FieldSimple,
    /// `w:smartTag`.
    SmartTag,
    /// `w:customXml`.
    CustomXml,
    /// `w:sdt`: a content control.
    Control,
    /// `w:dir`.
    Dir,
    /// `w:bdo`.
    Bdo,
    /// The branch of an `mc:AlternateContent`.
    Alternate,
    /// `w:ruby`.
    Ruby,
}

impl WrapperKind {
    /// True for a wrapper whose content exists only after the revision is accepted.
    pub fn is_insertion(self) -> bool {
        matches!(
            self,
            WrapperKind::Ins | WrapperKind::MoveTo | WrapperKind::RowIns | WrapperKind::CellIns
        )
    }

    /// True for a wrapper whose content is gone once the revision is accepted.
    pub fn is_deletion(self) -> bool {
        matches!(
            self,
            WrapperKind::Del | WrapperKind::MoveFrom | WrapperKind::RowDel | WrapperKind::CellDel
        )
    }
}

/// One wrapper element around some items, and the wrapper around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    /// The next wrapper out, if any.
    pub parent: Option<u32>,
    /// What the wrapper is.
    pub kind: WrapperKind,
    /// The wrapper element.
    pub node: NodeId,
}

/// A field as one paragraph saw it. A field that began in an earlier paragraph is carried in, and
/// has `begins_here` false.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldRef {
    /// The story-wide id: fields are numbered in document order of their start. It is an internal
    /// handle, renumbered when a field is added or removed, and is not stable across edits. A field
    /// is named across edits by its `begin` element.
    pub id: u32,
    /// The `w:fldChar` `begin`, or the `w:fldSimple`.
    pub begin: NodeId,
    /// True if this paragraph holds the start.
    pub begins_here: bool,
    /// The `w:fldChar` `separate`, if this paragraph holds it.
    pub separate: Option<NodeId>,
    /// The `w:fldChar` `end`, if this paragraph holds it.
    pub end: Option<NodeId>,
    /// True for a `w:fldSimple`.
    pub simple: bool,
    /// The instruction pieces this paragraph holds, joined.
    pub instr: String,
    /// The id of the field this one is nested in.
    pub parent: Option<u32>,
}

/// The text of one paragraph and the map from it to XML.
#[derive(Clone, Debug)]
pub struct TextMap {
    pub(super) text: String,
    pub(super) items: Vec<Item>,
    pub(super) contexts: Vec<Context>,
    pub(super) fields: Vec<FieldRef>,
    pub(super) anomalies: Vec<Anomaly>,
    pub(super) has_revisions: bool,
    /// The fields still open at the end of the paragraph, for the next one.
    pub(super) open_at_end: Vec<OpenField>,
    /// How many fields started in this paragraph.
    pub(super) began: u32,
}

impl TextMap {
    /// The text of the paragraph in a mode. Borrowed when no revision wrapper is in the paragraph.
    pub fn text(&self, mode: Mode) -> Cow<'_, str> {
        if mode == Mode::Redline || !self.has_revisions {
            return Cow::Borrowed(&self.text);
        }
        let mut out = String::with_capacity(self.text.len());
        for item in self
            .items
            .iter()
            .filter(|i| i.len > 0 && i.vis.in_mode(mode))
        {
            out.push_str(self.slice(item));
        }
        Cow::Owned(out)
    }

    /// The characters an item contributes to the Redline text.
    pub(super) fn slice(&self, item: &Item) -> &str {
        let from = item.byte as usize;
        self.text
            .get(from..from + item.blen as usize)
            .unwrap_or_default()
    }

    /// How many characters the paragraph has in a mode.
    pub fn len(&self, mode: Mode) -> usize {
        self.items
            .iter()
            .filter(|i| i.vis.in_mode(mode))
            .map(|i| i.len as usize)
            .sum()
    }

    /// True if the paragraph has no character in a mode.
    pub fn is_empty(&self, mode: Mode) -> bool {
        self.len(mode) == 0
    }

    /// The items, in XML order, marks included.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// True if some wrapper makes an item exist in one of `Final` and `Original` and not the other.
    pub fn has_revisions(&self) -> bool {
        self.has_revisions
    }

    /// The wrappers around an item, innermost first. Empty for [`NO_CONTEXT`].
    pub fn context(&self, ctx: u32) -> impl Iterator<Item = &Context> {
        let mut next = self.contexts.get(ctx as usize);
        std::iter::from_fn(move || {
            let current = next?;
            next = current.parent.and_then(|p| self.contexts.get(p as usize));
            Some(current)
        })
    }

    /// The fields this paragraph holds a part of.
    pub fn fields(&self) -> &[FieldRef] {
        &self.fields
    }

    /// What reading this paragraph skipped or repaired.
    pub fn anomalies(&self) -> &[Anomaly] {
        &self.anomalies
    }
}
