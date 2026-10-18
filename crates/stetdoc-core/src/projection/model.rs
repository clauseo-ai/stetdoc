//! The neutral model: what a reader needs and what an edit must preserve, in no format's syntax.
//!
//! A `Projection` is a head and a body. Every format renders it and no format owns it. It is plain
//! data (design 3): a paragraph is a list of inlines, and `draft` (in `draft_build.rs`) cuts that
//! list into the editable text slots and the read-only seams that an edit is compared by. This file
//! holds the head, the stories and the blocks, and `model_inline.rs` holds the inlines and the one
//! measure every other file needs, an inline's width in the `Final` text.
//!
//! Invariants that a builder must keep and the formats rely on: every ref in a model is unique, a
//! cell has at least one block, a row has at least one cell and a table at least one row, and every
//! `RevTag.author` indexes `Head.authors`. A model that breaks them still renders and parses, but
//! it does not describe a document.

use crate::api::view::{Mode, Position, Warning};
use crate::refs::{BaseToken, BlockRef};
use crate::wordml::{SectionBreak, StoryKind, VMerge};
use std::sync::Arc;

pub(crate) use super::model_inline::{
    Atom, AutoKind, ControlInfo, ControlState, Field, HiddenReason, Inline, LinkTarget,
    LooksHiddenSignal, Mark, NoteKind, ObjectInfo, ObjectKind, RevKind, RevTag, count, final_text,
    flatten, width_of,
};
pub(crate) use super::model_view::{
    AppendixEntry, ChangeList, Margin, MentionState, Outline, OutlineEntry, OutlineSource, Target,
    ThreadComment, ThreadEntry, ThreadList,
};

/// What a format renders: the head of a view or a file, and its body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Projection {
    /// The base token, the mode, the authors, the summary and the warnings.
    pub head: Head,
    /// The stories, or one of the other kinds of view.
    pub body: Body,
    /// The notes and comments that a page of a view cites. Empty for files.
    pub appendix: Vec<AppendixEntry>,
}

impl Projection {
    /// The stories of the body, or none when the body is another kind of view.
    pub(crate) fn stories(&self) -> &[Story] {
        match &self.body {
            Body::Stories(stories) => stories,
            Body::Outline(_) | Body::Lookup(_) | Body::Threads(_) | Body::Ledger(_) => &[],
        }
    }
}

/// What a file or a page says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Head {
    /// The namespace, state version and lineage the model was built at.
    pub base: BaseToken,
    /// How tracked changes appear. Readable files are always `Redline`.
    pub mode: Mode,
    /// Whether this is the readable files of a working copy or one page of a view.
    pub output: Output,
    /// The commonest paragraph style of the document: a paragraph in it carries no style tag.
    pub common_style: Option<Box<str>>,
    /// Every revision author of the document, in order of first appearance, with a short code.
    pub authors: Vec<Author>,
    /// Counts for the summary line.
    pub summary: Summary,
    /// What part of what a page is. `None` for readable files.
    pub position: Option<Position>,
    /// What the reader must know that is not in the text.
    pub warnings: Vec<Warning>,
}

impl Head {
    /// The author that a name is, as the head lists it. A change with no author is `Unknown`, as the document counts it.
    pub(crate) fn author_named(&self, name: Option<&str>) -> Option<&Author> {
        let wanted = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or("Unknown");
        self.authors.iter().find(|a| &*a.name == wanted)
    }
}

/// What a projection is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Output {
    /// The readable files of a working copy: `document`, `notes`, `headers` and `comments`.
    Files,
    /// One page of a view, written as one file. The name says which view: `body`, `outline`, and so on.
    View {
        /// The view's name.
        name: Box<str>,
    },
}

/// A revision author: the code the text shows and the name the head explains it with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Author {
    /// A short code such as `JD`: capitals of the name, a digit added to tell two apart.
    pub code: Box<str>,
    /// The name as the document has it.
    pub name: Box<str>,
}

/// The counts of the summary line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    /// Tracked changes by author, indexed like `Head.authors`.
    pub changes: Vec<u32>,
    /// Comments in all.
    pub comments: u32,
    /// Comments that are resolved.
    pub resolved: u32,
    /// Footnotes.
    pub footnotes: u32,
    /// Endnotes.
    pub endnotes: u32,
    /// Sections.
    pub sections: u32,
    /// Paragraphs that are read-only as a whole.
    pub readonly: u32,
}

/// The part of the model that differs between kinds of output. A new kind of view is a new variant,
/// which touches every format (design 3.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Body {
    /// Stories of blocks: a text view, or the readable files.
    Stories(Vec<Story>),
    /// The headings of the body, with what each covers.
    Outline(Outline),
    /// The answer to a lookup, a snapshot at one state. It is plain data and holds its own base token.
    Lookup(crate::api::lookup::Lookup),
    /// The comment threads, one entry each.
    Threads(ThreadList),
    /// The tracked changes, one line each, from `ledger`.
    Ledger(ChangeList),
}

/// One flow of content: the body, a header or footer part, the notes or the comments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Story {
    /// Which flow.
    pub kind: StoryKind,
    /// The part's name in the package.
    pub part: Arc<str>,
    /// For a header or footer: the sections that use this part, and as which slot.
    pub uses: Vec<SectionUse>,
    /// The body, a header or a footer is one unit. Notes and comments are one unit each.
    pub units: Vec<Unit>,
}

/// A section that uses a header or footer part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SectionUse {
    /// The section's number: the order of its `w:sectPr` in the body, from 1.
    pub section: u32,
    /// As which header or footer it is used.
    pub slot: Slot,
}

/// Which header or footer of a section a part is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Slot {
    /// `w:type="default"`.
    Default,
    /// `w:type="first"`.
    First,
    /// `w:type="even"`.
    Even,
}

/// A body, a header or footer, a note or a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Unit {
    /// What it is.
    pub kind: UnitKind,
    /// The `w:id` of a note or comment.
    pub id: Option<i64>,
    /// For a note, its number as the agent sees it, counted per kind from the first reference in the body, every reference
    /// counted: the id that refs and operations name it by (`fn3`), the same in every view.
    pub ordinal: u32,
    /// For a note, the number Word shows as its mark in this view, when it is not `ordinal`: the `Final` and `Original`
    /// views number as Word does once their changes are resolved, without the references they do not show.
    pub mark: Option<u32>,
    /// Author, date and thread of a comment.
    pub meta: UnitMeta,
    /// The blocks, in order.
    pub blocks: Vec<Block>,
}

/// What a unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnitKind {
    /// The body.
    Body,
    /// A header part.
    Header,
    /// A footer part.
    Footer,
    /// A footnote.
    Footnote,
    /// An endnote.
    Endnote,
    /// A comment.
    Comment,
}

/// What a comment adds to its unit.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct UnitMeta {
    /// The author, an index into `Head.authors`.
    pub author: Option<u32>,
    /// The author's initials as the document has them.
    pub initials: Option<Box<str>>,
    /// The date as written.
    pub date: Option<Arc<str>>,
    /// The `w:id` of the comment this one replies to.
    pub parent: Option<i64>,
    /// Whether the thread is resolved.
    pub resolved: bool,
    /// The block the comment is anchored on.
    pub on: Option<BlockRef>,
    /// The text the comment is anchored to, one string.
    pub anchored: Option<Box<str>>,
}

/// A block of a unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    /// A paragraph.
    Paragraph(Paragraph),
    /// A table.
    Table(Table),
    /// A section break, after the paragraph that ends the section.
    Section(SectionLine),
    /// Content the model cannot show, or a field result that is collapsed.
    Opaque(OpaqueBlock),
    /// A line in the margin of a page of a view, about the block before it. Never in files.
    Margin(Margin),
}

/// A paragraph: tags that are read-only, and its inlines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Paragraph {
    /// Its ref. `None` for a block that a dry run created and no commit has numbered.
    pub r#ref: Option<BlockRef>,
    /// The style's name, or `None` when it is the document's common style.
    pub style: Option<Box<str>>,
    /// The list label Word shows.
    pub label: Option<Label>,
    /// The paragraph mark was inserted, deleted or moved.
    pub mark: Option<MarkRev>,
    /// Other revisions anchored here: formatting and numbering changes.
    pub revs: Vec<PropRev>,
    /// Hidden, joined and read-only.
    pub flags: ParaFlags,
    /// The content.
    pub inlines: Vec<Inline>,
    /// The paragraph's text in `Final` as the document's text map gives it: the text that the offsets of an
    /// operation count in (`Edit::offsets` converts UTF-16 offsets from it). Only a JSON view fills it, so that a
    /// program can turn a position it found into an offset; files and other formats leave it `None`.
    pub final_text: Option<Box<str>>,
}

/// The label of a list paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Label {
    /// The number Word shows (`4.2`, `(a)`), or for a bullet its glyph.
    pub text: Box<str>,
    /// True for a bullet.
    pub bullet: bool,
    /// The label of a deleted paragraph, where it would have fallen.
    pub ghost: bool,
    /// The level's own run properties hide the label.
    pub hidden: bool,
}

/// A paragraph mark that was inserted, deleted or moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkRev {
    /// What happened to the mark.
    pub kind: MarkKind,
    /// The revision.
    pub tag: RevTag,
}

/// What happened to a paragraph mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkKind {
    /// Inserted.
    Inserted,
    /// Deleted.
    Deleted,
    /// Moved in.
    MovedTo,
    /// Moved away.
    MovedFrom,
}

/// A change anchored at a block that is not text: a property change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PropRev {
    /// What changed.
    pub kind: ChangeKind,
    /// The revision.
    pub tag: RevTag,
}

/// What a property change changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// Run formatting.
    Run,
    /// Paragraph formatting.
    Paragraph,
    /// Section properties.
    Section,
    /// Table properties.
    Table,
    /// The table grid.
    Grid,
    /// Row properties.
    Row,
    /// Cell properties.
    Cell,
    /// A list number.
    Numbering,
}

/// What a paragraph is, besides its text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParaFlags {
    /// Word hides the paragraph.
    pub hidden: bool,
    /// The paragraph's text continues in the next paragraph.
    pub joins_next: bool,
    /// Why the paragraph as a whole cannot be edited, if it cannot.
    pub readonly: Option<ReadOnly>,
}

/// Why a paragraph is read-only as a whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadOnly {
    /// Its mark and all its content are a tracked deletion.
    DeletedBlock,
    /// It is inside a content control that is locked.
    LockedControl,
    /// It is inside a content control that is bound to data.
    DataBound,
    /// It is a control's placeholder.
    Placeholder,
}

/// A table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Table {
    /// Its ref.
    pub r#ref: Option<BlockRef>,
    /// The style's name.
    pub style: Option<Box<str>>,
    /// The length of the grid.
    pub cols: u32,
    /// Property changes of the table.
    pub revs: Vec<PropRev>,
    /// The rows.
    pub rows: Vec<Row>,
}

/// A table row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Row {
    /// Its ref.
    pub r#ref: Option<BlockRef>,
    /// The row was inserted or deleted as a tracked change.
    pub change: Option<RowRev>,
    /// A header row.
    pub header: bool,
    /// Property changes of the row.
    pub revs: Vec<PropRev>,
    /// The cells, left to right.
    pub cells: Vec<Cell>,
}

/// A row that a tracked change inserted or deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RowRev {
    /// Inserted or deleted.
    pub kind: RowChange,
    /// The revision.
    pub tag: RevTag,
}

/// What a tracked change did to a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowChange {
    /// The row was inserted.
    Inserted,
    /// The row was deleted.
    Deleted,
}

/// A table cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    /// The grid column where the cell starts, from 0.
    pub col: u32,
    /// How many grid columns it spans.
    pub span: u32,
    /// Its place in a vertical merge.
    pub v_merge: VMerge,
    /// The cell was inserted, deleted or merged as a tracked change.
    pub change: Option<CellRev>,
    /// The blocks in the cell: at least one.
    pub blocks: Vec<Block>,
}

/// A cell that a tracked change inserted, deleted or merged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CellRev {
    /// What happened.
    pub kind: CellChange,
    /// The revision.
    pub tag: RevTag,
}

/// What a tracked change did to a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CellChange {
    /// The cell was inserted.
    Inserted,
    /// The cell was deleted.
    Deleted,
    /// The cell was merged.
    Merged,
}

/// The end of a section: the break after the paragraph that carries it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SectionLine {
    /// The section's number, from 1.
    pub index: u32,
    /// How the next section starts.
    pub break_type: SectionBreak,
    /// The next section has a different first page.
    pub title_pg: bool,
    /// The paragraph mark that carries the break was inserted or deleted.
    pub mark: Option<MarkRev>,
    /// Property changes of the section.
    pub revs: Vec<PropRev>,
}

/// Block-level content that has no paragraph form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OpaqueBlock {
    /// A `w:altChunk`: content imported from another part.
    AltChunk,
    /// An equation where a block stands, as its linear text.
    Math(Box<str>),
    /// Any other block-level element the reader does not know.
    Unknown,
    /// A field result over many paragraphs (a table of contents), collapsed.
    Collapsed {
        /// The field's keyword, in lower case.
        keyword: Box<str>,
        /// The first paragraph the result covers.
        first: Option<BlockRef>,
        /// The last paragraph the result covers.
        last: Option<BlockRef>,
        /// How many paragraphs it covers.
        blocks: u32,
    },
}
