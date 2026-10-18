//! The story index: the blocks of one part, in document order.
//!
//! A story is one part (the body, a header, a footer, the footnotes, the endnotes or the comments).
//! `Story::scan` (in `story_scan.rs`) walks its block structure once and returns this owned index. It
//! does not borrow the tree, so `state` can keep one per part. See `docs/design/wordml.md`, section 3.

use super::anomaly::Anomaly;
use super::text_fields::FieldIndex;
use crate::api::view::Mode;
use crate::xml::{NodeId, SpanTree};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Changes only in a major release: the rules of which blocks `Story::scan` lists, and their order.
pub const SCAN_VERSION: u32 = 1;

/// Changes only in a major release: the definition of a block fingerprint.
pub const FINGERPRINT_VERSION: u32 = 1;

/// Which flow of content a part holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StoryKind {
    /// The main document part, `w:document/w:body`.
    Body,
    /// A header part, `w:hdr`.
    Header,
    /// A footer part, `w:ftr`.
    Footer,
    /// The footnotes part, `w:footnotes`.
    Footnotes,
    /// The endnotes part, `w:endnotes`.
    Endnotes,
    /// The comments part, `w:comments`.
    Comments,
}

/// What a block is. A cell is not a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// A paragraph, `w:p`.
    Paragraph,
    /// A table, `w:tbl`.
    Table,
    /// A table row, `w:tr`.
    Row,
}

/// The `unit` of an entry that is in no note or comment.
pub const NO_UNIT: u32 = u32::MAX;

/// The paragraph holds a section break (`w:pPr/w:sectPr`).
pub const SECTION_BREAK: u8 = 1;
/// The paragraph mark is a tracked insertion (`w:pPr/w:rPr/w:ins`).
pub const MARK_INSERTED: u8 = 2;
/// The paragraph mark is a tracked deletion (`w:pPr/w:rPr/w:del`).
pub const MARK_DELETED: u8 = 4;
/// The paragraph mark was moved away (`w:pPr/w:rPr/w:moveFrom`).
pub const MARK_MOVED_FROM: u8 = 8;
/// The paragraph mark was moved in (`w:pPr/w:rPr/w:moveTo`).
pub const MARK_MOVED_TO: u8 = 16;
/// The block sits inside a block-level content control.
pub const IN_CONTROL: u8 = 32;

/// The tracked changes on a paragraph mark, `w:pPr/w:rPr`, or on a row, `w:trPr` ([`Row::revs`]). A
/// mark can hold more than one: an insertion by one author that another author deleted is a `w:ins`
/// and a `w:del` side by side, and the mark is gone in both modes. So read it through
/// [`MarkRevs::gone_in`], not as one kind.
///
/// [`Row::revs`]: super::Row::revs
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MarkRevs {
    /// `w:ins`: the mark is a tracked insertion.
    pub inserted: bool,
    /// `w:del`: the mark is a tracked deletion.
    pub deleted: bool,
    /// `w:moveFrom`: the mark was moved away.
    pub moved_from: bool,
    /// `w:moveTo`: the mark was moved in.
    pub moved_to: bool,
}

impl MarkRevs {
    /// What the `MARK_*` bits of a block entry say.
    pub fn from_flags(flags: u8) -> MarkRevs {
        MarkRevs {
            inserted: flags & MARK_INSERTED != 0,
            deleted: flags & MARK_DELETED != 0,
            moved_from: flags & MARK_MOVED_FROM != 0,
            moved_to: flags & MARK_MOVED_TO != 0,
        }
    }

    /// True if no revision is on the mark.
    pub fn is_none(&self) -> bool {
        *self == MarkRevs::default()
    }

    /// True if the mark does not exist in `mode`: it is deleted or moved away, which `Final` applies,
    /// or inserted or moved in, which `Original` rejects. `Redline` shows every mark. A paragraph whose
    /// mark is gone reads as one with the next ([`Story::join_run`]), where the next is in the same
    /// container.
    pub fn gone_in(&self, mode: Mode) -> bool {
        match mode {
            Mode::Final => self.deleted || self.moved_from,
            Mode::Original => self.inserted || self.moved_to,
            Mode::Redline => false,
        }
    }
}

/// One block of a story. Entries are in pre-order: a table, then each of its rows, then the blocks
/// of each row's cells, cell by cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockEntry {
    /// The block's element.
    pub node: NodeId,
    /// What kind of block it is.
    pub kind: BlockKind,
    /// The index of the enclosing entry: a table for a row, a row for the blocks in its cells.
    pub parent: Option<u32>,
    /// The index into [`Story::units`] of the enclosing note or comment, or [`NO_UNIT`].
    pub unit: u32,
    /// `SECTION_BREAK`, the `MARK_*` bits and `IN_CONTROL`, for a paragraph.
    pub flags: u8,
}

impl BlockEntry {
    /// True if every bit of `flag` is set.
    pub fn has(&self, flag: u8) -> bool {
        self.flags & flag == flag
    }

    /// The revisions on the paragraph mark, all of them. None for a block that is not a paragraph.
    pub fn mark_revs(&self) -> MarkRevs {
        MarkRevs::from_flags(self.flags)
    }
}

/// What a unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnitKind {
    /// A footnote.
    Footnote,
    /// An endnote.
    Endnote,
    /// A comment.
    Comment,
}

/// A note or a comment: a unit inside its story.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// The `w:id`, if it is a whole number.
    pub id: Option<i64>,
    /// The `w:footnote`, `w:endnote` or `w:comment` element.
    pub node: NodeId,
    /// Which of the three it is.
    pub kind: UnitKind,
    /// The comment's author, if it has one.
    pub author: Option<Box<str>>,
    /// The comment's initials, if it has any.
    pub initials: Option<Box<str>>,
    /// The comment's date as written, if it has one.
    pub date: Option<Box<str>>,
}

/// What an opaque block-level element is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OpaqueKind {
    /// A `w:altChunk`: content imported from another part.
    AltChunk,
    /// An `m:oMath` or `m:oMathPara` standing where a block stands.
    Math,
    /// Any other element the scan does not know where a block or a table part should be.
    Unknown,
}

/// Block-level content the model cannot read: not a block, so it has no ref.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opaque {
    /// The element.
    pub node: NodeId,
    /// What it is.
    pub kind: OpaqueKind,
    /// The index of the entry before it, so a placeholder can be shown in the right place.
    pub after: Option<usize>,
}

/// A block's identity within one document state: the part and the element. It is never persisted,
/// because a `NodeId` does not survive a re-parse: the persisted form is the ordinal in
/// [`Story::blocks`] and a fingerprint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockKey {
    part: Arc<str>,
    node: NodeId,
}

impl BlockKey {
    /// A key for the element `node` of the part `part`.
    pub fn new(part: Arc<str>, node: NodeId) -> BlockKey {
        BlockKey { part, node }
    }

    /// The part name, as `state` spelled it.
    pub fn part(&self) -> &Arc<str> {
        &self.part
    }

    /// The block's element.
    pub fn node(&self) -> NodeId {
        self.node
    }
}

/// The blocks of one story, in document order. An owned index: it does not borrow the tree, and is
/// valid for the tree it was scanned from as that tree was at scan time.
#[derive(Clone, Debug)]
pub struct Story {
    pub(super) part: Arc<str>,
    pub(super) kind: StoryKind,
    pub(super) entries: Vec<BlockEntry>,
    /// For each entry, a key of the wrappers above it (`context_key.rs`).
    pub(super) ctx: Vec<u64>,
    pub(super) units: Vec<Unit>,
    pub(super) opaque: Vec<Opaque>,
    pub(super) anomalies: Vec<Anomaly>,
    by_node: OnceLock<HashMap<NodeId, u32>>,
    /// The field pass, run the first time a paragraph's text is read, once for this story and tree.
    pub(super) fields: OnceLock<Arc<FieldIndex>>,
    /// `SpanTree::edit_count` of the tree the story was scanned from, as it was then.
    pub(super) edits: u64,
}

impl Story {
    pub(super) fn new(part: Arc<str>, kind: StoryKind) -> Story {
        Story {
            part,
            kind,
            entries: Vec::new(),
            ctx: Vec::new(),
            units: Vec::new(),
            opaque: Vec::new(),
            anomalies: Vec::new(),
            by_node: OnceLock::new(),
            fields: OnceLock::new(),
            edits: 0,
        }
    }

    /// True if `tree` has not been edited since this story was scanned from it: the editor has made
    /// no change to it, as `SpanTree::edit_count` counts them. The blocks the story lists and the
    /// field index it keeps describe the tree as it was at the scan, so on a tree that was edited
    /// since, `Story::paragraph`, `Story::block`, `Story::joined` give `None` and `Story::fields` and
    /// `Story::field_anomalies` give nothing: ask `Story::refresh` for the story of the new tree. The
    /// count tells a tree that changed from the one that was scanned, not two trees with the same
    /// number of edits apart.
    pub fn is_current(&self, tree: &SpanTree) -> bool {
        self.edits == tree.edit_count()
    }

    /// The part this story is.
    pub fn part(&self) -> &Arc<str> {
        &self.part
    }

    /// What kind of story it is.
    pub fn kind(&self) -> StoryKind {
        self.kind
    }

    /// The blocks, in pre-order and document order.
    pub fn blocks(&self) -> &[BlockEntry] {
        &self.entries
    }

    /// The identity of an entry.
    pub fn key(&self, entry: &BlockEntry) -> BlockKey {
        BlockKey::new(Arc::clone(&self.part), entry.node)
    }

    /// The index in [`Story::blocks`] of the block whose element is `node`.
    pub fn index_of(&self, node: NodeId) -> Option<usize> {
        let map = self.by_node.get_or_init(|| {
            self.entries
                .iter()
                .enumerate()
                .filter_map(|(i, e)| Some((e.node, u32::try_from(i).ok()?)))
                .collect()
        });
        map.get(&node).map(|&i| i as usize)
    }

    /// Block-level content the scan found and could not read, with `w:altChunk` among it.
    pub fn opaque(&self) -> &[Opaque] {
        &self.opaque
    }

    /// The notes or comments of this story, which [`BlockEntry::unit`] indexes.
    pub fn units(&self) -> &[Unit] {
        &self.units
    }

    /// What the scan skipped or repaired.
    pub fn anomalies(&self) -> &[Anomaly] {
        &self.anomalies
    }

    /// The unit an entry is in, if any.
    pub fn unit_of(&self, entry: &BlockEntry) -> Option<&Unit> {
        self.units.get(entry.unit as usize)
    }

    /// True if two stories list the same blocks, units, opaque content and anomalies. The caches
    /// a story fills for itself are not compared.
    pub fn same_as(&self, other: &Story) -> bool {
        self.part == other.part
            && self.kind == other.kind
            && self.entries == other.entries
            && self.ctx == other.ctx
            && self.units == other.units
            && self.opaque == other.opaque
            && self.anomalies == other.anomalies
    }
}
