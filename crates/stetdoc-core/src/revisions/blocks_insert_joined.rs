//! Insertions beside a joined container end (design `revisions.md`, 3.2.2): a paragraph whose mark is tracked-deleted,
//! followed by the container's last paragraph, which keeps its mark and has all its text tracked-deleted. That is what a
//! tracked delete of a container's last paragraph writes, and it is recognised from the document alone, whoever wrote it and
//! in whichever batch. The two read as one paragraph, the earlier one, so a paragraph inserted after the earlier one or
//! before the last one goes after the last one as a paragraph of its own (a Y at the container's end), and takes the
//! properties, the mark's run properties and the run formatting that the last paragraph had before the join, which the
//! join's property changes hold.

use super::blocks::mark_of;
use super::blocks_delete_last::mark_base;
use super::compiler::Compiler;
use super::names::is_w;
use super::props_edit::find_chain;
use super::resolve_mark::{next_paragraph, previous_paragraph};
use super::tree_ops::{detach, insert, move_to};
use super::types::PropElement;
use crate::api::error::StetError;
use crate::wordml::{BlockKey, Mode, NodeId};
use crate::xml::SpanTree;

impl Compiler<'_, '_> {
    /// True if `para` is the last paragraph of a joined container end: it ends its container (or an insertion of this batch
    /// beside it already put a paragraph after it), keeps its mark, has text that is all deleted, and the paragraph before it
    /// has a deleted mark.
    fn is_joined_end(&self, key: &BlockKey) -> Result<bool, StetError> {
        let shape = {
            let tree = self.tree_ro(key.part())?;
            let node = key.node();
            let mark = mark_of(tree, node);
            // An earlier insertion of this batch beside it left it with an inserted mark of this batch, and its own new
            // paragraph after it.
            let made_y = mark.ins.is_some_and(|i| self.is_created_node(tree, i));
            is_w(&tree.node_ref(node), "p")
                && !mark.is_deleted()
                && (made_y || super::required::ends_container(tree, node))
                && previous_paragraph(tree, node).is_some_and(|p| mark_of(tree, p).is_deleted())
        };
        if !shape {
            return Ok(false);
        }
        let map = self.paragraph(key)?.map;
        Ok(map.len(Mode::Final) == 0 && map.len(Mode::Original) > 0)
    }

    /// True if an insertion before `anchor` goes after it: `anchor` is the last paragraph of a joined container end, or an
    /// emptied paragraph that a mark this batch deleted joins on to.
    pub(super) fn takes_a_join(&self, anchor: &BlockKey) -> Result<bool, StetError> {
        if self.is_joined_end(anchor)? {
            return Ok(true);
        }
        let joins = {
            let tree = self.tree_ro(anchor.part())?;
            is_w(&tree.node_ref(anchor.node()), "p")
                && previous_paragraph(tree, anchor.node()).is_some_and(|prev| {
                    mark_of(tree, prev)
                        .del
                        .is_some_and(|d| self.is_created_node(tree, d))
                })
        };
        Ok(joins && self.paragraph(anchor)?.map.len(Mode::Final) == 0)
    }

    /// Where an insertion after `anchor` goes when `anchor` is a pending merge, a paragraph with text whose mark is
    /// deleted: after the paragraph the merge ends in, for a mark this batch deleted (the two read as one in the Final view),
    /// or, for any other, when that paragraph is the last of a joined container end. After another author's deleted mark
    /// before a paragraph with text, the new paragraph follows the anchor directly, as it did in 0.9.3.
    pub(super) fn join_end(&self, anchor: &BlockKey) -> Result<Option<BlockKey>, StetError> {
        let start = anchor.node();
        let (end, made_here) = {
            let tree = self.tree_ro(anchor.part())?;
            let mark = mark_of(tree, start);
            if !is_w(&tree.node_ref(start), "p") || !mark.is_deleted() {
                return Ok(None);
            }
            let made_here = mark.del.is_some_and(|d| self.is_created_node(tree, d));
            let mut cur = start;
            while mark_of(tree, cur).is_deleted() {
                match next_paragraph(tree, cur) {
                    Some(n) => cur = n,
                    None => break,
                }
            }
            (cur, made_here)
        };
        if end == start || self.paragraph(anchor)?.map.len(Mode::Final) == 0 {
            return Ok(None);
        }
        let key = BlockKey::new(anchor.part().clone(), end);
        Ok((made_here || self.is_joined_end(&key)?).then_some(key))
    }

    /// The last paragraph of the container from `anchor` on: an insertion before an emptied last paragraph goes after it, and
    /// after the paragraphs that earlier such insertions put there, so that they keep their order.
    pub(super) fn container_end(&self, anchor: &BlockKey) -> Result<BlockKey, StetError> {
        let tree = self.tree_ro(anchor.part())?;
        let mut cur = anchor.node();
        while let Some(n) = next_paragraph(tree, cur) {
            cur = n;
        }
        Ok(BlockKey::new(anchor.part().clone(), cur))
    }

    /// After an insertion that went after a joined container end as a Y: the new paragraph holds the last paragraph's
    /// original element properties, which the join aligned to the earlier paragraph's with property changes. The new
    /// paragraph takes what those changes hold, the properties the last paragraph had before the join, and the changes move to
    /// the paragraph the Y left with the inserted mark, which is the one that joins the earlier paragraph. The new text takes
    /// the mark's run properties, as an insertion before the last paragraph did in 0.9.3.
    pub(super) fn restore_original_props(
        &mut self,
        part: &std::sync::Arc<str>,
    ) -> Result<(), StetError> {
        let Some(new) = self.fx.new_blocks.first().map(|k| k.node()) else {
            return Ok(());
        };
        // The join wrote its alignment with the stamp of the mark it deleted, the mark of the paragraph before the last.
        let (left, stamp) = {
            let tree = self.tree_ro(part)?;
            let Some(left) = previous_paragraph(tree, new) else {
                return Ok(());
            };
            let stamp = previous_paragraph(tree, left)
                .and_then(|q| mark_of(tree, q).del)
                .and_then(|d| crate::wordml::revision_at(&tree.node_ref(d)))
                .map(|r| (r.author, r.date));
            let Some(stamp) = stamp else { return Ok(()) };
            (left, stamp)
        };
        let mut moved = false;
        for path in [&["pPr"][..], &["pPr", "rPr"][..]] {
            moved |= self.move_alignment(part, new, left, path, &stamp)?;
        }
        if !moved {
            return Ok(());
        }
        let tree = self.tx.part_mut(part)?;
        let mark: Vec<NodeId> = find_chain(tree, new, &["pPr", "rPr"])
            .map(|rpr| {
                tree.node_ref(rpr)
                    .child_elements()
                    .filter(|c| !is_revision(c.local_name()))
                    .map(|c| c.id())
                    .collect()
            })
            .unwrap_or_default();
        let runs: Vec<NodeId> = tree
            .node_ref(new)
            .child_elements()
            .filter(|c| is_w(c, "ins"))
            .flat_map(|ins| {
                ins.child_elements()
                    .filter(|r| is_w(r, "r"))
                    .map(|r| r.id())
                    .collect::<Vec<_>>()
            })
            .collect();
        for run in runs {
            let old = tree
                .node_ref(run)
                .child_elements()
                .find(|c| is_w(c, "rPr"))
                .map(|c| c.id());
            if let Some(old) = old {
                detach(tree, old)?;
            }
            if mark.is_empty() {
                continue;
            }
            let uri = super::names::wns(tree);
            let rpr = super::tree_ops::create_w(tree, &uri, "rPr")?;
            for m in &mark {
                let copy = tree.edit().clone_subtree(*m)?;
                insert(tree, rpr, None, copy)?;
            }
            let first = tree.node_ref(run).first_child().map(|c| c.id());
            insert(tree, run, first, rpr)?;
        }
        Ok(())
    }

    /// Moves the property change at `path` (`pPr`, or the mark's `pPr/rPr`) of `new` to `left`, when it is the join's
    /// alignment: it has `stamp`, the author and date of the join's mark deletion, and `left`'s properties there are what
    /// `new`'s are. The change then gives `left` what it changes from, and `new` takes the change's snapshot as its
    /// properties. Another author's change, or one made at another time, is left where it is.
    fn move_alignment(
        &mut self,
        part: &std::sync::Arc<str>,
        new: NodeId,
        left: NodeId,
        path: &[&str],
        stamp: &(Option<String>, Option<String>),
    ) -> Result<bool, StetError> {
        let change = if path.len() == 1 {
            "pPrChange"
        } else {
            "rPrChange"
        };
        let found = {
            let tree = self.tree_ro(part)?;
            let Some(container) = find_chain(tree, new, path) else {
                return Ok(false);
            };
            let Some(c) = tree
                .node_ref(container)
                .child_elements()
                .find(|c| is_w(c, change))
            else {
                return Ok(false);
            };
            let by_join =
                crate::wordml::revision_at(&c).is_some_and(|r| (r.author, r.date) == *stamp);
            if !by_join || base(tree, new, path) != base(tree, left, path) {
                return Ok(false);
            }
            let snapshot: Vec<NodeId> = c
                .child_elements()
                .next()
                .map(|s| s.child_elements().map(|e| e.id()).collect())
                .unwrap_or_default();
            let olds: Vec<NodeId> = tree
                .node_ref(container)
                .child_elements()
                .filter(|e| !keeps(e.local_name(), path))
                .map(|e| e.id())
                .collect();
            (container, c.id(), snapshot, olds)
        };
        let (container, c, snapshot, olds) = found;
        let tree = self.tx.part_mut(part)?;
        for o in olds {
            detach(tree, o)?;
        }
        // The snapshot's children come first, in their order, before what the container keeps (a mark's `w:rPr`, revisions).
        let first = tree.node_ref(container).first_child().map(|x| x.id());
        for s in snapshot {
            let copy = tree.edit().clone_subtree(s)?;
            insert(tree, container, first, copy)?;
        }
        let (target, _) = super::props_edit::ensure_chain(tree, left, path)?;
        move_to(tree, c, target, None)?;
        // A container left empty says nothing, and an empty `w:pPr` or mark `w:rPr` is a violation where it stands.
        if tree.node_ref(container).child_elements().next().is_none() {
            detach(tree, container)?;
            if path.len() == 2
                && let Some(ppr) = find_chain(tree, new, &["pPr"])
                && tree.node_ref(ppr).child_elements().next().is_none()
            {
                detach(tree, ppr)?;
            }
        }
        Ok(true)
    }
}

/// True for a revision or change-tracking element of a mark's `w:rPr`.
fn is_revision(local: &str) -> bool {
    matches!(local, "ins" | "del" | "moveFrom" | "moveTo")
        || super::policy::is_change_tracking(local)
}

/// True if the child `local` of the container at `path` stays when the container's properties are replaced: the mark's
/// `w:rPr`, a section break and changes in a `w:pPr`, and revisions in a mark's `w:rPr`.
fn keeps(local: &str, path: &[&str]) -> bool {
    if path.len() == 1 {
        matches!(local, "rPr" | "sectPr") || super::policy::is_change_tracking(local)
    } else {
        is_revision(local)
    }
}

/// The properties at `path` of `para`, without change tracking: its base properties, or its mark's run properties.
fn base(tree: &SpanTree, para: NodeId, path: &[&str]) -> Vec<PropElement> {
    if path.len() == 2 {
        return mark_base(tree, para);
    }
    super::blocks_delete::copied_base(tree, para)
}
