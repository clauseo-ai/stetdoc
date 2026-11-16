//! The tracked deletion of the last paragraph of a container (design `revisions.md`, 3.2.2). Word cannot accept a deleted
//! mark on a container's last paragraph: in the body the revision stays pending, and in a cell Word removes the whole table
//! (hand check, Word for Mac 16.114.4, issue #25). So the last paragraph keeps its mark, and its text is deleted together
//! with the mark of the paragraph before it, which is Word's own gesture: a tracked merge of the two, with the alignment
//! that makes the survivor keep the earlier paragraph's properties under either rule of 3.2.4.

use super::blocks::prop_element_of;
use super::compiler::Compiler;
use super::names::is_w;
use super::names::is_word_ns;
use super::outcome::KeptParagraph;
use super::policy::is_change_tracking;
use super::props_edit::find_chain;
use super::required::KeptReason;
use super::resolve_mark::next_paragraph;
use super::types::{PropEdit, PropElement};
use crate::api::error::{ErrorCode, StetError};
use crate::wordml::BlockKey;
use crate::wordml::{Mode, NodeId};
use crate::xml::SpanTree;

impl Compiler<'_, '_> {
    /// Deletes the mark of `prev` in place of the mark of `block`, the last paragraph of its container, whose text is
    /// already deleted. When the two cannot be merged, `block` is kept, emptied, and the receipt says why, unless `quiet`
    /// (`block` is not the paragraph the operation names, which is then deleted as far as it can be already).
    pub(super) fn delete_last_by_join(
        &mut self,
        block: &BlockKey,
        prev: NodeId,
        quiet: bool,
    ) -> Result<(), StetError> {
        let part = block.part().clone();
        let first = BlockKey::new(part.clone(), prev);
        let keep = |c: &mut Self, reason: KeptReason| {
            if !quiet {
                c.fx.kept.push(KeptParagraph {
                    block: block.clone(),
                    reason,
                });
            }
            Ok(())
        };
        // A pending merge between the two, a paragraph with text whose mark is deleted, already takes the last paragraph's
        // place: its text ends in it on accept, so nothing more is joined.
        if self.text_joins_into(&part, prev, block.node())? {
            return Ok(());
        }
        if self.paragraph(&first)?.map.len(Mode::Final) == 0 {
            return keep(self, KeptReason::EmptyBefore);
        }
        // The mark's run properties are part of what accepting keeps of the last paragraph: they are the earlier one's.
        let marks = {
            let tree = self.tree_ro(&part)?;
            (mark_base(tree, prev), mark_base(tree, block.node()))
        };
        if let Some(cause) = self.join_blocker(&part, prev, block.node(), marks.0 != marks.1)? {
            return keep(self, KeptReason::CannotJoin(cause));
        }
        match self.merge_inner(&first, block) {
            Ok(()) => {}
            Err(e) if matches!(e.code(), ErrorCode::ReadonlyRegion | ErrorCode::CrossBlock) => {
                return keep(self, KeptReason::CannotJoin(cause_of(e.message())));
            }
            Err(e) => return Err(e),
        }
        let (from, to) = marks;
        // A merge that joined for real (the earlier mark was the author's own insertion) keeps the earlier element.
        let attached = self.tx.parts().is_attached(block)?;
        if from != to && attached {
            let clear = to
                .iter()
                .filter(|c| !from.iter().any(|f| f.local == c.local && f.ns == c.ns))
                .map(|c| (c.ns.clone(), c.local.clone()))
                .collect();
            let edit = PropEdit { set: from, clear };
            let direct = self.own_mark(&part, block.node())?;
            self.edit_container(&part, block.node(), &["pPr", "rPr"], direct, &edit)?;
        }
        Ok(())
    }

    /// After the text of `para`, whose mark is already deleted, is deleted: when it joins on to the container's last
    /// paragraph and that one is emptied (its text deleted, its mark kept), the paragraph before `para` loses its mark in
    /// turn, so that the result is what deleting the two from the top would give.
    pub(super) fn join_emptied_end(
        &mut self,
        part: &std::sync::Arc<str>,
        para: NodeId,
    ) -> Result<(), StetError> {
        let (last, prev) = {
            let tree = self.tree_ro(part)?;
            let Some(last) = super::required::end_joined_from(tree, para) else {
                return Ok(());
            };
            (last, super::required::previous_to_join(tree, last))
        };
        let Some(prev) = prev else { return Ok(()) };
        let key = BlockKey::new(part.clone(), last);
        let map = self.paragraph(&key)?.map;
        if map.len(Mode::Final) > 0 || map.len(Mode::Original) == 0 {
            return Ok(());
        }
        self.delete_last_by_join(&key, prev, true)
    }

    /// True if a paragraph between `prev` and `last` (whose marks are deleted) has text in `Final`.
    fn text_joins_into(
        &self,
        part: &std::sync::Arc<str>,
        prev: NodeId,
        last: NodeId,
    ) -> Result<bool, StetError> {
        let mut between = Vec::new();
        {
            let tree = self.tree_ro(part)?;
            let mut cur = next_paragraph(tree, prev);
            while let Some(n) = cur.filter(|n| *n != last) {
                between.push(n);
                cur = next_paragraph(tree, n);
            }
        }
        for n in between {
            let key = BlockKey::new(part.clone(), n);
            if self.paragraph(&key)?.map.len(Mode::Final) > 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Why `prev` and `last` cannot be merged, in the words of a receipt, for the causes the merge, or the carrying of the
    /// mark's run properties, would refuse with: another author's pending change to the properties or the mark of either.
    fn join_blocker(
        &self,
        part: &std::sync::Arc<str>,
        prev: NodeId,
        last: NodeId,
        carries_mark: bool,
    ) -> Result<Option<&'static str>, StetError> {
        let Some(stamp) = self.stamp() else {
            return Ok(None);
        };
        let tree = self.tree_ro(part)?;
        let foreign = |para: NodeId| {
            find_chain(tree, para, &["pPr"])
                .and_then(|ppr| {
                    tree.node_ref(ppr)
                        .child_elements()
                        .find(|c| is_w(c, "pPrChange"))
                })
                .and_then(|c| crate::wordml::revision_at(&c))
                .is_some_and(|r| r.author.as_deref() != Some(stamp.author()))
        };
        let foreign_mark = |para: NodeId| {
            find_chain(tree, para, &["pPr", "rPr"])
                .and_then(|rpr| {
                    tree.node_ref(rpr)
                        .child_elements()
                        .find(|c| is_w(c, "rPrChange"))
                })
                .and_then(|c| crate::wordml::revision_at(&c))
                .is_some_and(|r| r.author.as_deref() != Some(stamp.author()))
        };
        Ok(if carries_mark && foreign_mark(last) {
            Some("its paragraph mark has another author's pending change to its formatting")
        } else if carries_mark && foreign_mark(prev) {
            Some(
                "the mark of the paragraph before it has another author's pending change to its formatting",
            )
        } else if foreign(last) {
            Some("it has another author's pending change to its properties")
        } else if foreign(prev) {
            Some("the paragraph before it has another author's pending change to its properties")
        } else {
            None
        })
    }
}

/// The cause of a refused merge, in the words of a receipt, from the refusal's message.
fn cause_of(message: &str) -> &'static str {
    if message.contains("field") {
        "the paragraph before it ends inside a field"
    } else if message.contains("already deleted") {
        "the mark of the paragraph before it is already a pending deletion"
    } else if message.contains("section") {
        "the paragraph before it ends a section"
    } else {
        "the paragraph before it is read-only"
    }
}

/// The run properties of a paragraph's mark, without its revisions and change tracking.
pub(super) fn mark_base(tree: &SpanTree, para: NodeId) -> Vec<PropElement> {
    fn strip(mut e: PropElement) -> PropElement {
        e.children = e
            .children
            .into_iter()
            .filter(|c| !(is_word_ns(&c.ns) && is_change_tracking(&c.local)))
            .map(strip)
            .collect();
        e
    }
    find_chain(tree, para, &["pPr", "rPr"])
        .map(|rpr| {
            tree.node_ref(rpr)
                .child_elements()
                .filter(|c| {
                    !(is_word_ns(c.namespace())
                        && (is_change_tracking(c.local_name())
                            || matches!(c.local_name(), "ins" | "del" | "moveFrom" | "moveTo")))
                })
                .map(|c| strip(prop_element_of(&c)))
                .collect()
        })
        .unwrap_or_default()
}
