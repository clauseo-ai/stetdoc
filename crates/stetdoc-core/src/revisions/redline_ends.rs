//! The ends of containers in a redline (design `compare.md`, 4.5 and 6.2), once a story is written.
//!
//! A container that ends with inserted paragraphs gets a live last mark, as Word needs: the paragraph before the run
//! (past the inserted paragraphs and the tables whose rows are all inserted, which rejecting removes first) takes the inserted
//! mark, beside a deleted one if it is a deleted copy, and the container's last paragraph keeps a live one, which no rejection
//! can remove. It keeps its own properties, with the earlier paragraph's in a `pPrChange` and an `rPrChange`, so that rejecting
//! joins them into a paragraph with the earlier one's properties, as it was.

use super::blocks::mark_of;
use super::compiler::Compiler;
use super::errors::internal;
use super::insert_run::write_revision_attrs;
use super::join::MARKERS;
use super::names::{is_blank, is_w, wns};
use super::outcome::Effect;
use super::props_edit::{
    ensure_chain, find_chain, held, held_by_snapshot, place, snapshot as snapshot_of,
};
use super::tree_ops::{child_element_ids, create_w, detach};
use crate::api::error::StetError;
use crate::wordml::{BlockKey, NodeId};
use std::sync::Arc;

/// What `Compiler::settle_end_marks` did.
pub(crate) struct EndMarks {
    /// The effect of the calls.
    pub effect: Effect,
    /// Each paragraph that took a container's inserted end mark, with the paragraph that ends the container.
    pub given: Vec<(NodeId, NodeId)>,
    /// The paragraphs that end a container with an inserted mark that no paragraph before them could take.
    pub orphans: Vec<NodeId>,
}

impl Compiler<'_, '_> {
    /// Gives every container of `part` that ends with a run of inserted paragraphs a live last mark (see the module's comment),
    /// once the whole story is written, so that the paragraph before the run is the one it ends up with. Returns, with the
    /// effect, each paragraph that took the inserted mark with the paragraph that ends the container, which rejecting joins it
    /// into, and the paragraphs that end a container with an inserted mark that no paragraph before them could take.
    pub(crate) fn settle_end_marks(&mut self, part: &Arc<str>) -> Result<EndMarks, StetError> {
        self.begin();
        let mut given = Vec::new();
        let mut orphans = Vec::new();
        let res = (|| {
            let ends: Vec<NodeId> = {
                let tree = self.tree_ro(part)?;
                let mut out = Vec::new();
                let mut stack = vec![tree.root()];
                while let Some(n) = stack.pop() {
                    if is_w(&n, "p") {
                        if mark_of(tree, n.id()).ins.is_some()
                            && crate::wordml::ends_container(tree, n.id())
                            && !super::resolve_mark::goes_whole(tree, n.id())
                        {
                            out.push(n.id());
                        }
                        continue;
                    }
                    stack.extend(n.child_elements());
                }
                out
            };
            for z in ends {
                let last = BlockKey::new(part.clone(), z);
                match self.end_mark_owner(&last)? {
                    Some(live) => {
                        self.give_the_end_mark(part, &live, z)?;
                        given.push((live.node(), z));
                    }
                    None => orphans.push(z),
                }
            }
            Ok(())
        })();
        match res {
            Ok(()) => Ok(EndMarks {
                effect: self.end(),
                given,
                orphans,
            }),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    /// The paragraph before the run of inserted paragraphs that `last` ends, if `last` is a paragraph whose mark was just
    /// inserted and that ends its container, and that paragraph's mark is clean or deleted (a deleted copy of the original, whose
    /// mark then carries both, as a mark one author inserted and another deleted does) and carries no section break.
    fn end_mark_owner(&self, last: &BlockKey) -> Result<Option<BlockKey>, StetError> {
        let tree = self.tree_ro(last.part())?;
        let z = last.node();
        if !is_w(&tree.node_ref(z), "p")
            || mark_of(tree, z).ins.is_none()
            || !crate::wordml::ends_container(tree, z)
        {
            return Ok(None);
        }
        // Back over the paragraphs whose marks are inserted and the tables whose rows all are, which rejecting removes before
        // it joins the marks, to the paragraph before them.
        let mut cur = tree.node_ref(z).prev_sibling();
        let mut live = None;
        while let Some(c) = cur {
            cur = c.prev_sibling();
            if !c.is_element()
                || is_blank(&c)
                || MARKERS.iter().any(|m| is_w(&c, m))
                || is_w(&c, "proofErr")
            {
                continue;
            }
            if is_w(&c, "p") && mark_of(tree, c.id()).ins.is_some() {
                continue;
            }
            if is_w(&c, "tbl") && all_rows_inserted(tree, c.id()) {
                continue;
            }
            if is_w(&c, "p") {
                live = Some(c.id());
            }
            break;
        }
        Ok(live
            .filter(|&live| {
                let m = mark_of(tree, live);
                m.ins.is_none()
                    && m.move_from.is_none()
                    && m.move_to.is_none()
                    && find_chain(tree, live, &["pPr", "sectPr"]).is_none()
            })
            .map(|live| BlockKey::new(last.part().clone(), live)))
    }

    /// Moves the inserted mark from `last`, which ends its container, to `live`: see the module's comment.
    fn give_the_end_mark(
        &mut self,
        part: &Arc<str>,
        live: &BlockKey,
        last: NodeId,
    ) -> Result<(), StetError> {
        // (path, change element): the snapshots are what `live` had before any change of its own.
        let steps: [(&[&str], &str); 2] = [(&["pPr"], "pPrChange"), (&["pPr", "rPr"], "rPrChange")];
        let mut snaps: Vec<NodeId> = Vec::new();
        {
            let tree = self.tx.part_mut(part)?;
            for (path, change) in steps {
                let snap = match find_chain(tree, live.node(), path) {
                    Some(own) => {
                        let old = tree
                            .node_ref(own)
                            .child_elements()
                            .find(|c| is_w(c, change))
                            .and_then(|c| c.child_elements().next().map(|s| s.id()));
                        match old {
                            Some(old) => tree.edit().clone_subtree(old)?,
                            None => snapshot_of(tree, own, change)?,
                        }
                    }
                    None => {
                        let uri = wns(tree);
                        create_w(tree, &uri, path.last().copied().unwrap_or("pPr"))?
                    }
                };
                snaps.push(snap);
            }
            // The mark of `last` is live again: it is the container's end.
            if let Some(rpr) = find_chain(tree, last, &["pPr", "rPr"]) {
                for k in child_element_ids(tree, rpr) {
                    if is_w(&tree.node_ref(k), "ins") {
                        detach(tree, k)?;
                    }
                }
            }
        }
        self.add_mark_revision(part, live.node(), "ins")?;
        self.touch(live);
        for ((path, change), snap) in steps.into_iter().zip(snaps) {
            let id = self.next_id()?;
            let stamp = self
                .stamp()
                .cloned()
                .ok_or_else(|| internal("a tracked change has no stamp"))?;
            let tree = self.tx.part_mut(part)?;
            let (own, made) = ensure_chain(tree, last, path)?;
            if held(tree, own, change) == held_by_snapshot(tree, snap) {
                if let Some(m) = made {
                    detach(tree, m)?;
                }
                continue;
            }
            let uri = wns(tree);
            let ch = create_w(tree, &uri, change)?;
            write_revision_attrs(tree, ch, &uri, id, &stamp)?;
            tree.edit().append_child(ch, snap)?;
            place(tree, own, ch)?;
        }
        let tree = self.tx.part_mut(part)?;
        for path in [&["pPr", "rPr"][..], &["pPr"][..]] {
            if let Some(n) = find_chain(tree, last, path)
                && tree.node_ref(n).child_elements().next().is_none()
            {
                detach(tree, n)?;
            }
        }
        self.touch(&BlockKey::new(part.clone(), last));
        Ok(())
    }
}

/// True if every row of a table says it is inserted (`w:trPr/w:ins`).
fn all_rows_inserted(tree: &crate::xml::SpanTree, table: NodeId) -> bool {
    let t = tree.node_ref(table);
    let rows: Vec<_> = t.child_elements().filter(|r| is_w(r, "tr")).collect();
    !rows.is_empty()
        && rows.iter().all(|r| {
            r.child_elements()
                .find(|c| is_w(c, "trPr"))
                .is_some_and(|p| p.child_elements().any(|c| is_w(&c, "ins")))
        })
}
