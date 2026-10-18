//! Resolving property changes (design `revisions.md`, 3B): accepting one removes the change element,
//! rejecting one puts the snapshot back in the container. What the snapshot does not hold stays: the
//! mark and the section break of a `w:pPr`, the header and footer references of a `w:sectPr`, and every
//! live revision. A snapshot's stale copies of the mark's `w:ins` and `w:del` (Word 14 wrote them) are
//! dropped.

use super::cleanup::prune_from;
use super::names::{is_w, is_word_ns};
use super::policy::is_change_tracking;
use super::props_edit::{in_snapshot, place, snapshot_ctx};
use super::resolve::Engine;
use super::tree_ops::{child_element_ids, detach, insert, parent_of};
use crate::api::error::StetError;
use crate::wordml::{NodeId, RevKind};

impl Engine<'_, '_> {
    /// Resolves a `*PrChange` element.
    pub(super) fn resolve_props(&mut self, change: NodeId, kind: RevKind) -> Result<(), StetError> {
        let tree = self.tx.part_mut(&self.part)?;
        let Some(container) = parent_of(tree, change) else {
            return Ok(());
        };
        if self.accept || kind == RevKind::NumberingChange {
            detach(tree, change)?;
            let kept = prune_from(tree, container)?;
            self.dirty.push(kept);
            return Ok(());
        }
        if kind == RevKind::TableGrid {
            restore_grid(tree, change, container)?;
        } else {
            restore(tree, change, container)?;
        }
        let kept = prune_from(tree, container)?;
        self.dirty.push(kept);
        Ok(())
    }

    /// Resolves the `w:ins` of a `w:numPr`: accepting removes it, rejecting removes the whole `w:numPr`.
    pub(super) fn resolve_numpr_ins(&mut self, ins: NodeId) -> Result<(), StetError> {
        let tree = self.tx.part_mut(&self.part)?;
        let Some(numpr) = parent_of(tree, ins) else {
            return Ok(());
        };
        let Some(ppr) = parent_of(tree, numpr) else {
            return Ok(());
        };
        let kept = if self.accept {
            detach(tree, ins)?;
            prune_from(tree, numpr)?
        } else {
            detach(tree, numpr)?;
            prune_from(tree, ppr)?
        };
        self.dirty.push(kept);
        Ok(())
    }
}

/// Puts the snapshot of `change` back in `container`, and removes the change.
fn restore(
    tree: &mut crate::xml::SpanTree,
    change: NodeId,
    container: NodeId,
) -> Result<(), StetError> {
    let change_local = tree.node_ref(change).local_name().to_owned();
    let ctx = snapshot_ctx(tree, container, &change_local);
    let snap = tree
        .node_ref(change)
        .child_elements()
        .find(|c| c.local_name() == tree.node_ref(container).local_name())
        .map(|c| c.id());
    let owned: Vec<NodeId> = tree
        .node_ref(container)
        .child_elements()
        .filter(|c| c.id() != change && in_snapshot(c, ctx))
        .map(|c| c.id())
        .collect();
    for id in owned {
        detach(tree, id)?;
    }
    if let Some(snap) = snap {
        let mut placed: Vec<NodeId> = Vec::new();
        for id in child_element_ids(tree, snap) {
            let stale = {
                let n = tree.node_ref(id);
                is_word_ns(n.namespace()) && is_change_tracking(n.local_name())
            };
            detach(tree, id)?;
            if stale {
                continue;
            }
            // A snapshot that holds a property twice (a tool wrote it so, tdf104649) gets both back: the
            // schema has no slot for the second, so it goes right after its twin.
            match last_twin(tree, &placed, id) {
                Some(twin) => insert_after(tree, container, twin, id)?,
                None => place(tree, container, id)?,
            }
            placed.push(id);
        }
    }
    detach(tree, change)?;
    Ok(())
}

/// The last element of `placed` with the expanded name of `id`.
fn last_twin(tree: &crate::xml::SpanTree, placed: &[NodeId], id: NodeId) -> Option<NodeId> {
    let n = tree.node_ref(id);
    placed.iter().rev().copied().find(|&p| {
        let t = tree.node_ref(p);
        t.local_name() == n.local_name() && t.namespace() == n.namespace()
    })
}

/// Inserts the detached element `new` into `parent` right after its child `after`.
fn insert_after(
    tree: &mut crate::xml::SpanTree,
    parent: NodeId,
    after: NodeId,
    new: NodeId,
) -> Result<(), StetError> {
    let kids = child_element_ids(tree, parent);
    let next = kids
        .iter()
        .position(|&k| k == after)
        .and_then(|i| kids.get(i + 1).copied());
    match next {
        Some(before) => insert(tree, parent, Some(before), new),
        None => tree.edit().append_child(parent, new),
    }
}

/// The old grid: the `w:gridCol` children of the snapshot replace the live ones.
fn restore_grid(
    tree: &mut crate::xml::SpanTree,
    change: NodeId,
    container: NodeId,
) -> Result<(), StetError> {
    let snap = tree
        .node_ref(change)
        .child_elements()
        .find(|c| is_w(c, "tblGrid"))
        .map(|c| c.id());
    let cols: Vec<NodeId> = tree
        .node_ref(container)
        .child_elements()
        .filter(|c| is_w(c, "gridCol"))
        .map(|c| c.id())
        .collect();
    for id in cols {
        detach(tree, id)?;
    }
    if let Some(snap) = snap {
        for id in child_element_ids(tree, snap) {
            detach(tree, id)?;
            place(tree, container, id)?;
        }
    }
    detach(tree, change)?;
    Ok(())
}
