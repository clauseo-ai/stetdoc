//! The tracked deletion of the last paragraph of a container (design `revisions.md`, 3.2.2). Word cannot accept a deleted
//! mark on a container's last paragraph: in the body the revision stays pending, and in a cell Word removes the whole table
//! (hand check, Word for Mac 16.114.4, issue #25). So the last paragraph keeps its mark, and its text is deleted together
//! with the mark of the paragraph before it, which is Word's own gesture: a tracked merge of the two, with the alignment
//! that makes the survivor keep the earlier paragraph's properties under either rule of 3.2.4.

use super::blocks::mark_of;
use super::compiler::Compiler;
use super::names::is_w;
use super::outcome::KeptParagraph;
use super::required::KeptReason;
use super::resolve_mark::next_paragraph;
use crate::api::error::{ErrorCode, StetError};
use crate::wordml::BlockKey;
use crate::wordml::{Mode, NodeId};

impl Compiler<'_, '_> {
    /// Deletes the mark of `prev` in place of the mark of `block`, the last paragraph of its container, whose text is
    /// already deleted. When the two cannot be merged (`prev` is read-only, ends inside a field or has another author's
    /// pending change to its properties), `block` is kept, emptied, and the receipt says why.
    pub(super) fn delete_last_by_join(
        &mut self,
        block: &BlockKey,
        prev: NodeId,
    ) -> Result<(), StetError> {
        let first = BlockKey::new(block.part().clone(), prev);
        match self.merge_inner(&first, block) {
            Ok(()) => Ok(()),
            Err(e) if matches!(e.code(), ErrorCode::ReadonlyRegion | ErrorCode::CrossBlock) => {
                self.fx.kept.push(KeptParagraph {
                    block: block.clone(),
                    reason: KeptReason::CannotJoin,
                });
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// The paragraph that a paragraph with live text and a mark this batch deleted, a pending merge, joins into, which is where an
    /// insertion after it goes: in the Final view the two read as one, and a new paragraph right after the first would take
    /// its text. `None` when `anchor` is not such a paragraph.
    pub(super) fn join_end(&self, anchor: &BlockKey) -> Result<Option<BlockKey>, StetError> {
        let start = anchor.node();
        let end = {
            let tree = self.tree_ro(anchor.part())?;
            // Only a deletion this batch made: after a mark deleted earlier, the new paragraph follows the anchor directly,
            // as it did in 0.9.3, and accepting joins the anchor into it.
            let made_here = mark_of(tree, start)
                .del
                .is_some_and(|d| self.is_created_node(tree, d));
            if !is_w(&tree.node_ref(start), "p") || !made_here {
                return Ok(None);
            }
            let mut cur = start;
            while mark_of(tree, cur).is_deleted() {
                match next_paragraph(tree, cur) {
                    Some(n) => cur = n,
                    None => break,
                }
            }
            cur
        };
        if end == start || self.paragraph(anchor)?.map.len(Mode::Final) == 0 {
            return Ok(None);
        }
        Ok(Some(BlockKey::new(anchor.part().clone(), end)))
    }
}
