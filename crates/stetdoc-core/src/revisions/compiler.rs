//! The compiler: `Compiler` wraps a `Tx` and a `Track`, and every primitive of the module is a method
//! of it (design `revisions.md`, 2.2). This file holds the struct and what every primitive shares:
//! starting and ending a call, taking ids, and reading a paragraph through `wordml`. The primitives are
//! in `text.rs`, `props.rs`, `blocks_insert.rs`, `blocks_delete.rs`, `tables.rs`, `lists.rs` and
//! `markers.rs`, and each is an `impl Compiler` block there.

use super::errors::{cross_block, internal, stale};
use super::outcome::{Effect, Outcome};
use super::stamp::{RevStamp, Track};
use crate::api::error::StetError;
use crate::state::Tx;
use crate::wordml::BlockKey;
use crate::wordml::{BlockKind, Cell, NodeId, RevKind, TextMap};
use std::ops::Range;
use std::sync::Arc;

/// One transaction's edits, as one author's or untracked.
pub(crate) struct Compiler<'t, 's> {
    pub(super) tx: &'t mut Tx<'s>,
    pub(super) track: Track,
    /// The effect of the call in progress.
    pub(super) fx: Effect,
    pub(super) effects: Vec<Effect>,
    warnings: Vec<String>,
    /// The minted range when the call began.
    call_start: Range<u32>,
}

/// A paragraph read for an edit: everything the primitives need, owned, so that the transaction can
/// be borrowed mutably afterwards.
pub(super) struct ParaView {
    /// The part the paragraph is in.
    pub part: Arc<str>,
    /// The paragraph element.
    pub node: NodeId,
    /// Its index among the blocks of its story.
    pub index: usize,
    /// The `MARK_*` and `SECTION_BREAK` bits of its entry.
    pub flags: u8,
    /// The text model of the paragraph.
    pub map: TextMap,
    /// True if the paragraph sits in a row or a cell that a tracked change deletes.
    pub in_deleted_row_or_cell: bool,
}

impl<'t, 's> Compiler<'t, 's> {
    /// A compiler over `tx`. `Track::Untracked` writes no revision and takes no id.
    pub(crate) fn new(tx: &'t mut Tx<'s>, track: Track) -> Result<Compiler<'t, 's>, StetError> {
        Ok(Compiler {
            tx,
            track,
            fx: Effect::default(),
            effects: Vec::new(),
            warnings: Vec::new(),
            call_start: 0..0,
        })
    }

    /// The transaction, for `Tx::resolve` between two calls of one batch.
    pub(crate) fn tx(&self) -> &Tx<'s> {
        self.tx
    }

    /// A handle that writes the parts that are not the document's stories: the comment parts, the style sheet
    /// and `people.xml`, which hold no revisions. It refuses a story, so the compiler stays the one writer of
    /// those. The compiler reads the transaction afresh at every call, so it stays true to what the handle wrote.
    pub(crate) fn parts_mut(&mut self) -> super::handle::PartWriter<'_, 's> {
        super::handle::PartWriter::over(&mut *self.tx)
    }

    /// Takes out the footnotes and endnotes that lost their last reference in this transaction (an untracked
    /// deletion of a note), and returns them.
    pub(crate) fn remove_unreferenced_notes(
        &mut self,
        rejected: bool,
    ) -> Result<Vec<super::notes::RemovedNote>, StetError> {
        super::notes::remove_unreferenced_notes(&mut *self.tx, rejected)
    }

    /// The stamp, if edits are tracked.
    pub(super) fn stamp(&self) -> Option<&RevStamp> {
        match &self.track {
            Track::Tracked(s) => Some(s),
            Track::Untracked | Track::UntrackedBy(_) => None,
        }
    }

    /// True if edits write revisions.
    pub(crate) fn tracked(&self) -> bool {
        matches!(self.track, Track::Tracked(_))
    }

    /// Notes something worth telling the agent that is not an error.
    pub(super) fn warn(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    /// Begins a call: a fresh effect, and the minted range to measure the call's own ids from.
    pub(super) fn begin(&mut self) {
        self.fx = Effect::default();
        self.call_start = self.tx.minted_annotation_ids();
    }

    /// Ends a call: records the ids it took, keeps the effect for the outcome and returns it.
    pub(super) fn end(&mut self) -> Effect {
        let now = self.tx.minted_annotation_ids();
        self.fx.minted = if self.call_start.is_empty() {
            now
        } else {
            self.call_start.end..now.end
        };
        let fx = std::mem::take(&mut self.fx);
        self.effects.push(fx.clone());
        fx
    }

    /// The next annotation id, for a revision this call writes.
    pub(super) fn next_id(&mut self) -> Result<u32, StetError> {
        self.tx.next_annotation_id()
    }

    /// Records that a pending revision was cut in two, or copied: the new id is a piece of a revision
    /// that existed, which the transaction remembers for the compilers that come after this one.
    pub(super) fn note_split(&mut self, old: i64, new: i64) -> Result<(), StetError> {
        let id =
            u32::try_from(new).map_err(|_| internal("a copied revision has an id out of range"))?;
        self.tx.note_copied_annotation_id(id)?;
        self.fx.split.push((old, new));
        Ok(())
    }

    /// Reads the paragraph a key names, through `wordml`, as the transaction sees it. A block that is
    /// not in the document any more is `stale_ref`, and one that is a table or a row is `cross_block`.
    pub(super) fn paragraph(&self, block: &BlockKey) -> Result<ParaView, StetError> {
        let view = self.tx.parts();
        let part = view
            .id(block.part())
            .ok_or_else(|| internal(format!("{} is not a part", block.part())))?;
        let story = view.story(part)?;
        let tree = view.tree(part)?;
        let index = story.index_of(block.node()).ok_or_else(|| {
            stale(format!(
                "a block of {} is not in the document any more",
                block.part()
            ))
        })?;
        let entry = *story
            .blocks()
            .get(index)
            .ok_or_else(|| internal("a block index is out of range"))?;
        if entry.kind != BlockKind::Paragraph {
            return Err(cross_block(
                "this block is a table or a row, and the operation needs a paragraph",
            ));
        }
        let paragraph = story
            .paragraph(tree, index)
            .ok_or_else(|| internal("a paragraph entry has no paragraph"))?;
        let map = paragraph.map();
        Ok(ParaView {
            part: Arc::clone(block.part()),
            node: block.node(),
            index,
            flags: entry.flags,
            map,
            in_deleted_row_or_cell: in_deleted_row_or_cell(tree, block.node()),
        })
    }

    /// The tree of a part as the transaction sees it, to read. It stages nothing.
    pub(super) fn tree_ro(&self, part: &str) -> Result<&crate::xml::SpanTree, StetError> {
        let view = self.tx.parts();
        let id = view
            .id(part)
            .ok_or_else(|| internal(format!("{part} is not a part")))?;
        view.tree(id)
    }

    /// The effects of the calls that have ended.
    pub(super) fn effects_so_far(&self) -> &[Effect] {
        &self.effects
    }

    /// Touches a block: its own bytes changed.
    pub(super) fn touch(&mut self, block: &BlockKey) {
        self.tx.touch(block);
        if !self.fx.touched.contains(block) {
            self.fx.touched.push(block.clone());
        }
    }

    /// Ends the compiler: derives what the transaction created and checks it (`verify.rs`).
    pub(crate) fn finish(self) -> Result<Outcome, StetError> {
        let created = super::verify::created(&self)?;
        super::verify::check_created(&self, &created)?;
        let outcome = Outcome {
            minted: self.tx.minted_annotation_ids(),
            effects: self.effects,
            created,
            warnings: self.warnings,
        };
        Ok(outcome)
    }
}

/// True if the paragraph is in a row or a cell that a tracked change deletes.
pub(super) fn in_deleted_row_or_cell(tree: &crate::xml::SpanTree, node: NodeId) -> bool {
    let mut cur = tree.node_ref(node).parent();
    while let Some(n) = cur {
        // A row can hold both an insertion and a deletion (two authors), so every revision is read.
        if super::names::is_w(&n, "tr")
            && n.child_elements()
                .find(|c| super::names::is_w(c, "trPr"))
                .is_some_and(|pr| pr.child_elements().any(|c| super::names::is_w(&c, "del")))
        {
            return true;
        }
        if let Some(cell) = Cell::new(tree, n.id())
            && cell.revisions().any(|r| r.kind == RevKind::CellDel)
        {
            return true;
        }
        cur = n.parent();
    }
    false
}
