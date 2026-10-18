//! `delete_block` for paragraphs and `merge_blocks` (design `revisions.md`, 3.2.2 and 3.2.3).
//!
//! Deleting a paragraph deletes its content as `replace_text` does, and then its mark, when a paragraph
//! follows it and it holds no section break. A merge deletes the first paragraph's mark and aligns the
//! second paragraph's properties to the first's, so that accepting gives the same answer under either
//! rule for which paragraph's properties survive a join (3.2.4). A merge that is real, because the mark is
//! the author's own insertion or the edit is untracked, is the join of 6.2.

use super::blocks::{container_of, mark_of, opaque_after, prop_element_of, same_parent};
use super::compiler::{Compiler, ParaView};
use super::errors::{cross_block, readonly};
use super::join::{Joined, Why, join, leftovers, ppr_revision_ids};
use super::names::is_w;
use super::outcome::{Effect, Origin, RemovedKind, RemovedOwn};
use super::props::Direct;
use super::props_edit::find_chain;
use super::text::final_starts;
use super::tree_ops::detach;
use super::types::{PropEdit, PropElement};
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{BlockKind, Mode, NodeId, Paragraph, RevKind, SECTION_BREAK};
use crate::xml::{NodeRef, SpanTree};

/// The edit that makes the base properties of Q those of P: P's are set, and what only Q has is cleared.
fn alignment_edit(tree: &SpanTree, p: NodeId, q: NodeId) -> PropEdit {
    let base = |para: NodeId| -> Vec<NodeRef<'_>> {
        find_chain(tree, para, &["pPr"])
            .map(|ppr| {
                tree.node_ref(ppr)
                    .child_elements()
                    .filter(|c| !matches!(c.local_name(), "rPr" | "sectPr" | "pPrChange"))
                    .collect()
            })
            .unwrap_or_default()
    };
    let from: Vec<PropElement> = base(p).iter().map(prop_element_of).collect();
    let clear = base(q)
        .iter()
        .filter(|c| {
            !from
                .iter()
                .any(|f| f.local == c.local_name() && f.ns == c.namespace())
        })
        .map(|c| (c.namespace().to_owned(), c.local_name().to_owned()))
        .collect();
    PropEdit { set: from, clear }
}

impl Compiler<'_, '_> {
    /// Deletes a paragraph, a row or a table.
    pub(crate) fn delete_block(&mut self, block: &BlockKey) -> Result<Effect, StetError> {
        self.delete_block_inner(block, false)
    }

    /// Deletes a paragraph as `delete_block` does, and, tracked, its mark as well when no paragraph follows
    /// it in its container. This is the shape of a note whose whole body is deleted (`RP050`, 3.6.4): every
    /// paragraph of the note loses its mark, the last one too, though nothing follows it. A paragraph with a
    /// paragraph after it, and a row or a table, are deleted as `delete_block` deletes them, and an
    /// untracked deletion is the same as `delete_block`'s. The note's own number (`w:footnoteRef`, `w:endnoteRef`) is deleted
    /// with its text, as Word writes it in `RP050`: Word removes the note when the deletion of its reference is accepted only if
    /// nothing of the note is left live (hand check 26, 2026-10-10).
    pub(crate) fn delete_block_with_mark(&mut self, block: &BlockKey) -> Result<Effect, StetError> {
        self.delete_block_inner(block, true)
    }

    fn delete_block_inner(
        &mut self,
        block: &BlockKey,
        force_mark: bool,
    ) -> Result<Effect, StetError> {
        self.begin();
        let kind = {
            let sibs = self.sibs(block);
            match sibs {
                Ok(s) => s.entry(s.index).map(|e| e.kind),
                Err(e) => {
                    self.fx = Effect::default();
                    return Err(e);
                }
            }
        };
        let res = match kind {
            Some(BlockKind::Paragraph) => self.delete_paragraph(block, force_mark, !force_mark),
            Some(BlockKind::Table | BlockKind::Row) => self.delete_table_block(block),
            None => Err(super::errors::stale(
                "the block is not in the document any more",
            )),
        };
        match res {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    /// Deletes a paragraph: its content, then its mark. `force_mark` deletes the mark even when no
    /// paragraph follows, for the paragraphs of a row that goes as a whole. `keep_marks` keeps a note's own number, as every
    /// deletion but that of a whole note does.
    pub(super) fn delete_paragraph(
        &mut self,
        block: &BlockKey,
        force_mark: bool,
        keep_marks: bool,
    ) -> Result<(), StetError> {
        let part = block.part().clone();
        let p = self.paragraph(block)?;
        // Untracked, the paragraph goes with everything in it, so nothing in it may be another author's pending change.
        self.refuse_foreign_within(&part, p.node, "this paragraph holds")?;
        let mark = mark_of(self.tree_ro(&part)?, p.node);
        let len = p.map.len(Mode::Final);
        let removed_row = p.in_deleted_row_or_cell;
        if (mark.is_deleted() || removed_row) && len == 0 {
            self.warn(
                "this paragraph is already deleted by a tracked change, so nothing was written",
            );
            return Ok(());
        }
        if len > 0 {
            let (planned, pieces) = self.plan_text(block, 0..len, "", keep_marks)?;
            let starts = final_starts(&planned.map);
            for piece in pieces.iter().rev() {
                self.apply_piece(&planned, &starts, piece)?;
            }
        } else {
            self.check_editable(&p)?;
        }
        self.touch(block);
        let sibs = self.sibs(block)?;
        let next_is_paragraph = {
            let tree = self.tree_ro(&part)?;
            sibs.next.and_then(|i| sibs.entry(i)).is_some_and(|e| {
                e.kind == BlockKind::Paragraph && same_parent(tree, p.node, e.node)
            })
        };
        let (section, mark_now) = {
            let tree = self.tree_ro(&part)?;
            (
                find_chain(tree, p.node, &["pPr", "sectPr"]).is_some(),
                mark_of(tree, p.node),
            )
        };
        if !self.tracked() {
            return self.untracked_remove(block, &p, section, next_is_paragraph);
        }
        if section || !(next_is_paragraph || force_mark) {
            return Ok(());
        }
        // A mark deleted right before content that is not a block (a `w:altChunk`) reads as joining the
        // paragraph with the one after that content, and accepting it cannot: the mark stays (R8).
        if opaque_after(self.tree_ro(&part)?, &sibs.story, p.node, None) {
            return Ok(());
        }
        if mark_now.is_deleted() {
            return Ok(());
        }
        let own = match mark_now.ins {
            Some(ins) => self.own_insertion(self.tree_ro(&part)?, ins),
            None => Direct::No,
        };
        if own == Direct::No {
            return self.add_mark_revision(&part, p.node, "del");
        }
        // The mark is the author's own insertion: it goes with the paragraph's content.
        self.remove_own_paragraph(block, &p, mark_now.ins)
    }

    /// Removes the mark of a paragraph that is the author's own insertion and whose content has been
    /// deleted. With a paragraph after it, the two are joined for real (6.2): the paragraph keeps its
    /// element and its ref, ends with the later paragraph's properties and mark, and takes the later one's
    /// content, so whatever is left of its own text (a pending deletion, another author's revisions) is
    /// still in a paragraph that has its ref, and the later paragraph's ref is merged into it. A paragraph
    /// with nothing but range markers and comment references is removed instead, and the markers go to
    /// the paragraph after it. With nothing after it, it goes only if a paragraph before it is left to end
    /// the container, as a cell and a body must end with one.
    fn remove_own_paragraph(
        &mut self,
        block: &BlockKey,
        p: &ParaView,
        ins: Option<NodeId>,
    ) -> Result<(), StetError> {
        let part = block.part().clone();
        let (ins_id, swallowed, left) = {
            let tree = self.tree_ro(&part)?;
            let id = ins
                .and_then(|i| crate::wordml::revision_at(&tree.node_ref(i)))
                .and_then(|r| r.id);
            let others: Vec<i64> = ppr_revision_ids(tree, p.node)
                .into_iter()
                .filter(|(k, _)| *k != RevKind::MarkIns)
                .map(|(_, id)| id)
                .collect();
            (id, others, leftovers(tree, p.node))
        };
        let sibs = self.sibs(block)?;
        // A paragraph at the end of a content control or a wrapper has no next paragraph to join, since the
        // resolver joins siblings only.
        let next = {
            let tree = self.tree_ro(&part)?;
            sibs.next
                .and_then(|i| sibs.entry(i))
                .filter(|e| e.kind == BlockKind::Paragraph && same_parent(tree, p.node, e.node))
        };
        if let Some(next) = next {
            // What is declared is what is done.
            self.fx.removed_own.push(RemovedOwn {
                id: ins_id,
                kind: if left.is_some() {
                    RemovedKind::Paragraph
                } else {
                    RemovedKind::Mark
                },
            });
            let joined = join(self.tx, &part, p.node, next.node, Why::Merge)?;
            self.fx.swallowed.extend(swallowed);
            let second = BlockKey::new(part.clone(), next.node);
            if joined == Joined::Merged {
                self.fx.survivor = Some(block.clone());
                self.fx.origins.push(Origin {
                    block: block.clone(),
                    from: vec![block.clone(), second.clone()],
                });
            }
            // The markers of a paragraph that went are in the next one now, and one that was joined
            // into is the survivor.
            self.touch(&second);
            return Ok(());
        }
        match left {
            Some(leftover)
                if leftover.is_empty()
                    && super::resolve_mark::previous_paragraph(self.tree_ro(&part)?, p.node)
                        .is_some() =>
            {
                self.fx.removed_own.push(RemovedOwn {
                    id: ins_id,
                    kind: RemovedKind::Paragraph,
                });
                let tree = self.tx.part_mut(&part)?;
                detach(tree, p.node)?;
                self.fx.swallowed.extend(swallowed);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// The untracked end of `delete_block`: the paragraph is removed, unless it carries a section break
    /// or is the last of its container, or holds content that is not its own text (another author's
    /// pending deletion), in which case it is emptied and kept.
    fn untracked_remove(
        &mut self,
        block: &BlockKey,
        p: &ParaView,
        section: bool,
        next_is_paragraph: bool,
    ) -> Result<(), StetError> {
        if section || !next_is_paragraph {
            return Ok(());
        }
        let part = block.part().clone();
        let sibs = self.sibs(block)?;
        let Some(next) = sibs.next.and_then(|i| sibs.entry(i)) else {
            return Ok(());
        };
        let left = leftovers(self.tree_ro(&part)?, p.node);
        let Some(left) = left else { return Ok(()) };
        let tree = self.tx.part_mut(&part)?;
        let gap = super::join::markers_between(tree, p.node, next.node);
        let moved = super::join::place_leftovers(tree, p.node, left, next.node, gap)?;
        detach(tree, p.node)?;
        if moved {
            self.touch(&BlockKey::new(part.clone(), next.node));
        }
        Ok(())
    }

    /// Joins two neighbouring paragraphs into one: the first's text and the second's.
    pub(crate) fn merge_blocks(
        &mut self,
        first: &BlockKey,
        second: &BlockKey,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.merge_inner(first, second) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn merge_inner(&mut self, first: &BlockKey, second: &BlockKey) -> Result<(), StetError> {
        let part = first.part().clone();
        if second.part() != first.part() {
            return Err(cross_block(
                "these paragraphs are in different parts, so they cannot be merged",
            ));
        }
        let sf = self.sibs(first)?;
        let ss = self.sibs(second)?;
        let (ef, es) = (self.expect_block(&sf)?, self.expect_block(&ss)?);
        if ef.kind != BlockKind::Paragraph || es.kind != BlockKind::Paragraph {
            return Err(cross_block(
                "a table or a row cannot be merged. Name two neighbouring paragraphs.",
            ));
        }
        // Neighbours in `Final`: the same container, in order, with only deleted blocks between.
        let tree = self.tree_ro(&part)?;
        if container_of(tree, first.node()) != container_of(tree, second.node())
            || !same_parent(tree, first.node(), second.node())
            || ss.index <= sf.index
        {
            return Err(cross_block(
                "these paragraphs are not neighbours in one container, so they cannot be merged. A merge joins a paragraph with the one right after it.",
            ));
        }
        for i in sf.index + 1..ss.index {
            let Some(e) = sf.entry(i) else { continue };
            let same = container_of(tree, e.node) == container_of(tree, first.node());
            if same && !self.is_deleted_entry(&part, &e)? {
                return Err(cross_block(
                    "there is a paragraph or a table between these two, so they are not neighbours. Merge a paragraph with the one right after it.",
                ));
            }
        }
        // Content that is not a block, such as a `w:altChunk`, is skipped when the text is read, so the
        // paragraphs read as joined across it; accepting the merge would join the first with that content
        // instead, so `Final` and accept all would disagree (R8).
        let opaque_between = opaque_after(tree, &sf.story, first.node(), Some(second.node()));
        if opaque_between {
            return Err(cross_block(
                "content that is not a paragraph or a table, such as an imported document part, stands between these two, so they cannot be merged. Edit the two paragraphs without merging them.",
            ));
        }
        let p = self.paragraph(first)?;
        let q = self.paragraph(second)?;
        self.check_editable(&p)?;
        self.check_editable(&q)?;
        if p.flags & SECTION_BREAK != 0 || q.flags & SECTION_BREAK != 0 {
            return Err(readonly(
                "this paragraph ends a section, and merging it would move or drop the break. Edit the two paragraphs without merging them.",
            ));
        }
        let fields_out = self.field_depth_after(&p, p.map.items().len().checked_sub(1)) > 0;
        if fields_out {
            return Err(readonly(
                "this paragraph begins or ends a field that spans paragraphs, so its mark cannot be merged. Edit the paragraphs without merging them.",
            ));
        }
        // The second may begin a field that runs on past it, such as a table of contents: the paragraphs the
        // field spans are one read-only block, and the merge would move this paragraph's text into it.
        let fields_on = self.field_depth_after(&q, q.map.items().len().checked_sub(1)) > 0;
        if fields_on {
            return Err(readonly(
                "the next paragraph begins a field that spans paragraphs, such as a table of contents, whose result is read-only, so it cannot take this paragraph's text. Edit the paragraphs without merging them.",
            ));
        }
        if let Some(stamp) = self.stamp() {
            let tree = self.tree_ro(&part)?;
            for para in [p.node, q.node] {
                let foreign = find_chain(tree, para, &["pPr"])
                    .and_then(|ppr| {
                        tree.node_ref(ppr)
                            .child_elements()
                            .find(|c| is_w(c, "pPrChange"))
                    })
                    .and_then(|c| crate::wordml::revision_at(&c))
                    .is_some_and(|r| r.author.as_deref() != Some(stamp.author()));
                if foreign {
                    return Err(readonly(
                        "this paragraph has a pending change to its properties by another author, so merging could not be resolved right. Accept or reject that change first.",
                    ));
                }
            }
        }
        // Untracked, the merge is done for real: the second mark goes and the properties move, so neither paragraph's properties
        // may hold another author's pending change.
        if !self.tracked() {
            for para in [p.node, q.node] {
                let ppr = find_chain(self.tree_ro(&part)?, para, &["pPr"]);
                if let Some(ppr) = ppr {
                    self.refuse_foreign_within(&part, ppr, "this paragraph's properties hold")?;
                }
            }
        }
        let mark = mark_of(self.tree_ro(&part)?, p.node);
        if mark.is_deleted() {
            return Err(readonly(
                "the mark of this paragraph is already deleted by a tracked change, so the merge is already pending. Accept or reject that change first.",
            ));
        }
        let own = match mark.ins {
            Some(ins) if self.tracked() => self.own_insertion(self.tree_ro(&part)?, ins),
            _ => Direct::No,
        };
        let real = !self.tracked() || own != Direct::No;
        // The alignment: Q's base properties become P's, as a change of Q's.
        let live = p.map.len(Mode::Final) > 0;
        let differ = {
            let tree = self.tree_ro(&part)?;
            let base = |n: NodeId| Paragraph::new(tree, n).map(|x| x.base_props());
            base(p.node) != base(q.node)
        };
        if live && differ {
            let edit = alignment_edit(self.tree_ro(&part)?, p.node, q.node);
            let direct = match self.tracked() {
                false => Direct::No,
                true => self.own_mark(&part, q.node)?,
            };
            self.edit_container(&part, q.node, &["pPr"], direct, &edit)?;
        }
        if !real {
            self.add_mark_revision(&part, p.node, "del")?;
            if live && differ {
                // Nothing is joined yet, but the second paragraph has the first one's properties now, so it
                // is built from both. (A merge that joins declares the survivor instead, below.)
                self.fx.origins.push(Origin {
                    block: second.clone(),
                    from: vec![second.clone(), first.clone()],
                });
            }
            self.touch(first);
            self.touch(second);
            return Ok(());
        }
        // A real merge: the join of 6.2.
        let (ins_id, swallowed) = {
            let tree = self.tree_ro(&part)?;
            let all = ppr_revision_ids(tree, p.node);
            let ins = all
                .iter()
                .find(|(k, _)| *k == RevKind::MarkIns)
                .map(|(_, id)| *id);
            let rest: Vec<i64> = all
                .into_iter()
                .filter(|(k, _)| *k != RevKind::MarkIns)
                .map(|(_, id)| id)
                .collect();
            (ins, rest)
        };
        if own != Direct::No {
            self.fx.removed_own.push(RemovedOwn {
                id: ins_id,
                kind: RemovedKind::Mark,
            });
            self.fx.swallowed.extend(swallowed);
        }
        // The first element survives, even when it holds nothing: the merge names it as the one that
        // remains, and so does the ref it keeps. The mark that goes is the author's own insertion, so it
        // joins the first paragraph with the one right after it, whose mark then stands in its place. When
        // deleted paragraphs stand between the two that the merge names (a range deleted across a paragraph
        // the batch inserted), that is the first of them: nothing moves across them, and the Original keeps
        // its order, while the Final still reads the two as one through their deleted marks.
        let partner = {
            let tree = self.tree_ro(&part)?;
            super::resolve_mark::next_paragraph(tree, p.node).unwrap_or(q.node)
        };
        let partner_key = BlockKey::new(part.clone(), partner);
        join(self.tx, &part, p.node, partner, Why::Keep)?;
        self.fx.survivor = Some(first.clone());
        self.fx.origins.push(Origin {
            block: first.clone(),
            from: vec![first.clone(), partner_key.clone()],
        });
        self.touch(first);
        self.touch(&partner_key);
        if partner != q.node {
            self.touch(second);
        }
        Ok(())
    }
}
