//! Resolving the range markers that Word puts around a content control to say it was inserted, deleted or
//! moved (design `revisions.md`, 3B and 3.6.2). The control's own text is not wrapped, so the markers are the
//! whole revision. For an inserted control, accepting removes the markers and rejecting unwraps the control,
//! whose content stays. For a deleted control it is the other way round. A moved control is read by its side:
//! the source as a deletion and the destination as an insertion.

use super::cleanup::prune_from;
use super::names::is_w;
use super::resolve::Engine;
use super::tree_ops::{child_ids, detach, move_to, parent_of};
use crate::api::error::StetError;
use crate::wordml::{NodeId, RevKind};
use crate::xml::SpanTree;

/// The `w:sdt` that a start marker stands in front of, or the one it is the first child of.
fn control_of(tree: &SpanTree, marker: NodeId) -> Option<NodeId> {
    let n = tree.node_ref(marker);
    let mut next = n.next_sibling();
    while let Some(c) = next {
        if is_w(&c, "sdt") {
            return Some(c.id());
        }
        // Only other markers may stand between.
        if c.is_element()
            && !c.local_name().ends_with("RangeStart")
            && !c.local_name().ends_with("RangeEnd")
        {
            break;
        }
        next = c.next_sibling();
    }
    let parent = n.parent()?;
    if is_w(&parent, "sdtContent") {
        return parent.parent().filter(|g| is_w(g, "sdt")).map(|g| g.id());
    }
    None
}

/// Replaces a control by what is in its `w:sdtContent`.
fn unwrap_control(tree: &mut SpanTree, sdt: NodeId) -> Result<NodeId, StetError> {
    let parent = parent_of(tree, sdt)
        .ok_or_else(|| super::errors::internal("a content control has no parent"))?;
    let content = tree
        .node_ref(sdt)
        .child_elements()
        .find(|c| is_w(c, "sdtContent"))
        .map(|c| c.id());
    if let Some(content) = content {
        for c in child_ids(tree, content) {
            move_to(tree, c, parent, Some(sdt))?;
        }
    }
    detach(tree, sdt)?;
    Ok(parent)
}

impl Engine<'_, '_> {
    /// Resolves one of the markers around a control.
    pub(super) fn resolve_control(
        &mut self,
        marker: NodeId,
        kind: RevKind,
    ) -> Result<(), StetError> {
        let start = matches!(
            kind,
            RevKind::CustomXmlInsStart
                | RevKind::CustomXmlDelStart
                | RevKind::CustomXmlMoveFromStart
                | RevKind::CustomXmlMoveToStart
        );
        let inserted = matches!(
            kind,
            RevKind::CustomXmlInsStart
                | RevKind::CustomXmlInsEnd
                | RevKind::CustomXmlMoveToStart
                | RevKind::CustomXmlMoveToEnd
        );
        let tree = self.tx.part_mut(&self.part)?;
        if start && inserted != self.accept {
            // Accepted deletion or rejected insertion: the control goes, its content stays.
            if let Some(sdt) = control_of(tree, marker) {
                let parent = unwrap_control(tree, sdt)?;
                self.dirty.push(parent);
            }
        }
        let tree = self.tx.part_mut(&self.part)?;
        if let Some(parent) = parent_of(tree, marker) {
            detach(tree, marker)?;
            let kept = prune_from(tree, parent)?;
            self.dirty.push(kept);
        }
        Ok(())
    }
}
