//! Deleting what a carve covered: wrap it in a `w:del`, or remove it (design `revisions.md`, 4.1).
//!
//! A group is a run of consecutive siblings of one parent. Whose text it is decides what is written.
//! Plain text, another author's insertion and a moved-in range get our `w:del` around the group, inside
//! whatever wraps it, so another author's `w:ins` stays one element that holds our `w:del`. Our own
//! pending insertion is removed outright, with its range markers kept in place, as Word does.

use super::compiler::{Compiler, ParaView};
use super::errors::internal;
use super::gap::Boundary;
use super::insert_run::write_revision_attrs;
use super::names::{is_w, rev_id, wns};
use super::outcome::{RemovedKind, RemovedOwn};
use super::tree_ops::{
    child_ids, create_w, detach, ensure_preserve, insert, move_to, next_sibling_of, parent_of,
    rename,
};
use crate::api::error::StetError;
use crate::wordml::{NodeId, WrapperKind};
use crate::xml::SpanTree;
use std::sync::Arc;

/// Whose text a group is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class {
    /// Not inside any insertion: delete it with a `w:del`.
    Plain,
    /// Inside the author's own pending insertion of text, a row or a cell: remove it.
    Own {
        /// The revision element it is inside: the `w:ins`, or the row's or cell's insertion marker.
        wrapper: NodeId,
    },
    /// Inside another author's insertion, or a moved-in range of anyone: delete it with a nested `w:del`.
    Nested,
}

impl Compiler<'_, '_> {
    /// Whose text the item `item` of the paragraph is.
    pub(super) fn class_of(&self, p: &ParaView, item: usize) -> Result<Class, StetError> {
        let Some(it) = p.map.items().get(item) else {
            return Err(internal("a group names an item that the map does not have"));
        };
        let tree = self.tree_ro(&p.part)?;
        let Some(owner) = p.map.context(it.ctx).find(|c| c.kind.is_insertion()) else {
            return Ok(Class::Plain);
        };
        let Some(stamp) = self.stamp() else {
            return Ok(Class::Nested);
        };
        let node = tree.node_ref(owner.node);
        let author = match owner.kind {
            WrapperKind::Ins | WrapperKind::MoveTo => {
                crate::wordml::revision_at(&node).and_then(|r| r.author)
            }
            WrapperKind::RowIns => crate::wordml::Row::new(tree, owner.node)
                .and_then(|r| {
                    r.revisions()
                        .find(|r| r.kind == crate::wordml::RevKind::RowIns)
                })
                .and_then(|r| r.author),
            WrapperKind::CellIns => crate::wordml::Cell::new(tree, owner.node)
                .and_then(|c| {
                    c.revisions()
                        .find(|r| r.kind == crate::wordml::RevKind::CellIns)
                })
                .and_then(|r| r.author),
            _ => None,
        };
        let own = author.as_deref() == Some(stamp.author());
        // A moved-in range is half of a move: it is never removed outright, whoever moved it.
        Ok(if own && owner.kind != WrapperKind::MoveTo {
            Class::Own {
                wrapper: owner.node,
            }
        } else {
            Class::Nested
        })
    }

    /// Deletes the groups, in order. Tracked, a group is wrapped in a `w:del` (or joins the one this
    /// transaction made just before it) unless it is the author's own pending text, which is removed.
    /// Untracked, every group is removed. Returns the boundary right after the last group.
    pub(super) fn delete_groups(
        &mut self,
        p: &ParaView,
        groups: Vec<(Vec<NodeId>, usize)>,
    ) -> Result<Option<Boundary>, StetError> {
        let mut last = None;
        for (nodes, item) in groups {
            let class = self.class_of(p, item)?;
            let boundary = if self.tracked() && class == Class::Plain
                || self.tracked() && class == Class::Nested
            {
                self.wrap_in_del(&p.part, &nodes)?
            } else {
                self.remove_group(&p.part, &nodes, class)?
            };
            last = Some(boundary);
        }
        Ok(last)
    }

    /// Wraps a group in `w:del`, and renames the text in it to what a deletion holds. Returns the
    /// boundary right after the wrapper.
    fn wrap_in_del(&mut self, part: &Arc<str>, nodes: &[NodeId]) -> Result<Boundary, StetError> {
        let Some(&first) = nodes.first() else {
            return Err(internal("a group of nodes to delete is empty"));
        };
        // Join the deletion just before the group, if this transaction made it and only markers stand
        // between.
        let (parent, joined) = {
            let tree = self.tree_ro(part)?;
            let parent = parent_of(tree, first).ok_or_else(|| internal("a unit has no parent"))?;
            let mut cur = tree.node_ref(first).prev_sibling();
            let mut found = None;
            while let Some(c) = cur {
                if super::carve::is_transparent(&c) {
                    cur = c.prev_sibling();
                } else {
                    if is_w(&c, "del") && self.is_created_node(tree, c.id()) {
                        found = Some(c.id());
                    }
                    break;
                }
            }
            (parent, found)
        };
        let del = match joined {
            Some(prev) => {
                // The markers between the old deletion and the group go into it, with the group.
                let tree = self.tx.part_mut(part)?;
                let between: Vec<NodeId> = {
                    let mut out = Vec::new();
                    let mut cur = tree.node_ref(prev).next_sibling();
                    while let Some(c) = cur {
                        if c.id() == first {
                            break;
                        }
                        out.push(c.id());
                        cur = c.next_sibling();
                    }
                    out
                };
                for m in between {
                    move_to(tree, m, prev, None)?;
                }
                prev
            }
            None => {
                let id = self.next_id()?;
                let stamp = self
                    .stamp()
                    .cloned()
                    .ok_or_else(|| internal("a tracked deletion has no stamp"))?;
                let wns_uri = wns(self.tree_ro(part)?);
                let tree = self.tx.part_mut(part)?;
                let del = create_w(tree, &wns_uri, "del")?;
                write_revision_attrs(tree, del, &wns_uri, id, &stamp)?;
                insert(tree, parent, Some(first), del)?;
                del
            }
        };
        let tree = self.tx.part_mut(part)?;
        for &n in nodes {
            move_to(tree, n, del, None)?;
        }
        for &n in nodes {
            rename_deleted_text(tree, n)?;
        }
        let after = next_sibling_of(tree, del);
        Ok(Boundary {
            parent,
            before: after,
        })
    }

    /// Removes a group outright, keeping range markers and the comment references in place. A wrapper
    /// that is left with nothing but markers is removed, and the markers go out before it. Returns the
    /// boundary where the group was.
    fn remove_group(
        &mut self,
        part: &Arc<str>,
        nodes: &[NodeId],
        class: Class,
    ) -> Result<Boundary, StetError> {
        let (Some(&first), Some(&last)) = (nodes.first(), nodes.last()) else {
            return Err(internal("a group of nodes to remove is empty"));
        };
        let (parent, after_last) = {
            let tree = self.tree_ro(part)?;
            (
                parent_of(tree, first).ok_or_else(|| internal("a unit has no parent"))?,
                next_sibling_of(tree, last),
            )
        };
        let wrapper_id = match class {
            Class::Own { wrapper } => {
                let tree = self.tree_ro(part)?;
                own_revision_id(tree, wrapper)
            }
            Class::Plain | Class::Nested => None,
        };
        let tree = self.tx.part_mut(part)?;
        let mut swallowed: Vec<i64> = Vec::new();
        for &n in nodes {
            let keep = {
                let node = tree.node_ref(n);
                super::carve::is_transparent(&node) || holds_only_markers(&node)
            };
            if !keep {
                // Revisions of other authors in what goes (a formatting change on the run, a deletion
                // inside the insertion) go with it, and the outcome says so.
                swallowed.extend(super::cleanup::revision_ids_in(tree, n));
                detach(tree, n)?;
            }
        }
        self.fx.swallowed.extend(swallowed);
        if matches!(class, Class::Own { .. }) {
            self.fx.removed_own.push(RemovedOwn {
                id: wrapper_id,
                kind: RemovedKind::Text,
            });
        }
        // An insertion wrapper that has nothing left goes, and its markers go out before it.
        let tree = self.tx.part_mut(part)?;
        let mut boundary = Boundary {
            parent,
            before: after_last,
        };
        if let Some((grand, next)) = drop_wrapper_if_empty(tree, parent)? {
            boundary = Boundary {
                parent: grand,
                before: next,
            };
        }
        Ok(boundary)
    }
}

/// Removes a revision wrapper that holds nothing but markers and white space, after moving the markers
/// out in front of it. Returns the parent and the next sibling where it was, or `None` if it holds
/// something and stays.
pub(super) fn drop_wrapper_if_empty(
    tree: &mut SpanTree,
    wrapper: NodeId,
) -> Result<Option<(NodeId, Option<NodeId>)>, StetError> {
    let removable = {
        let n = tree.node_ref(wrapper);
        (is_w(&n, "ins") || is_w(&n, "moveTo") || is_w(&n, "del") || is_w(&n, "moveFrom"))
            && n.children()
                .all(|c| super::carve::is_transparent(&c) || !c.is_element())
    };
    if !removable {
        return Ok(None);
    }
    let grand = parent_of(tree, wrapper).ok_or_else(|| internal("a wrapper has no parent"))?;
    for c in child_ids(tree, wrapper) {
        if tree.node_ref(c).is_element() {
            move_to(tree, c, grand, Some(wrapper))?;
        }
    }
    let next = next_sibling_of(tree, wrapper);
    detach(tree, wrapper)?;
    Ok(Some((grand, next)))
}

/// The id of the own revision that a wrapper context node stands for: a `w:ins`'s own id, or the id of
/// the row's or cell's insertion marker.
fn own_revision_id(tree: &SpanTree, node: NodeId) -> Option<i64> {
    let n = tree.node_ref(node);
    if is_w(&n, "ins") || is_w(&n, "moveTo") {
        return rev_id(&n);
    }
    if is_w(&n, "tr") {
        return crate::wordml::Row::new(tree, node)?
            .revisions()
            .find(|r| r.kind == crate::wordml::RevKind::RowIns)?
            .id;
    }
    if is_w(&n, "tc") {
        return crate::wordml::Cell::new(tree, node)?
            .revisions()
            .find(|r| r.kind == crate::wordml::RevKind::CellIns)?
            .id;
    }
    None
}

/// True for a run that holds nothing but a comment reference, which a removal keeps.
fn holds_only_markers(n: &crate::xml::NodeRef<'_>) -> bool {
    if !is_w(n, "r") {
        return false;
    }
    let mut content = n.child_elements().filter(|c| !is_w(c, "rPr"));
    let mut any = false;
    for c in &mut content {
        if !is_w(&c, "commentReference") {
            return false;
        }
        any = true;
    }
    any
}

/// Renames the text elements that a deletion holds: `w:t` becomes `w:delText` and `w:instrText`
/// becomes `w:delInstrText`, in the runs of the node.
pub(super) fn rename_deleted_text(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    let n = tree.node_ref(node);
    if !is_w(&n, "r") {
        return Ok(());
    }
    let targets: Vec<(NodeId, &'static str)> = n
        .child_elements()
        .filter_map(|c| {
            if is_w(&c, "t") {
                Some((c.id(), "delText"))
            } else if is_w(&c, "instrText") {
                Some((c.id(), "delInstrText"))
            } else {
                None
            }
        })
        .collect();
    for (el, name) in targets {
        let renamed = rename(tree, el, name)?;
        // The text may have been written without `xml:space` by a tool that dropped it for text that is
        // only white space, which a deletion may not do (W2).
        ensure_preserve(tree, renamed)?;
    }
    Ok(())
}
