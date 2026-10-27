//! Editing a property container: where a child goes, the snapshot of a `*PrChange`, and whether two
//! containers hold the same properties (design `revisions.md`, 2.2 `set_props` steps 3 to 5).
//!
//! Where a child goes is the schema's to say (`schema::insert_in_order`), and a container that is out of
//! order is rewritten in schema order when it changes (`Fit::Reorder`), so this file keeps no list of
//! properties and no order of its own.

use super::errors::invalid;
use super::names::{is_w, is_word_ns, wns};
use super::policy::is_change_tracking;
use super::tree_ops::{child_element_ids, create_w, detach, insert};
use super::types::PropEdit;
use crate::api::error::StetError;
use crate::schema::{Ctx, Fit, PartKind, Prefer, ctx_at, insert_in_order, reorder};
use crate::xml::{NodeId, NodeRef, SpanTree};

/// The kind of part a tree is, from its root element.
pub(super) fn part_kind(tree: &SpanTree) -> PartKind {
    match tree.root().local_name() {
        "hdr" => PartKind::Header,
        "ftr" => PartKind::Footer,
        "footnotes" => PartKind::Footnotes,
        "endnotes" => PartKind::Endnotes,
        "comments" => PartKind::Comments,
        "numbering" => PartKind::Numbering,
        "styles" => PartKind::Styles,
        _ => PartKind::Document,
    }
}

/// The declaration of the children of `node`, if the schema has one there. Below a paragraph, a table or a row
/// that stands where the schema does not place one (Word's own body-level `w:moveTo` around paragraphs, an
/// unknown wrapper), the walk from the root finds none; the children of that paragraph or table still have
/// the order the schema gives them anywhere, so the context is taken from the innermost one, as if it stood
/// in the body. Without it a new `w:pPr` would be appended after the runs, out of order the moment the
/// wrapper goes (when a move is accepted). A row that is being built, not yet in its table, is the same:
/// its cells' properties keep their order.
pub(super) fn ctx_of(tree: &SpanTree, node: NodeId) -> Option<Ctx> {
    let kind = part_kind(tree);
    if let Some(ctx) = ctx_at(kind, tree, node) {
        return Some(ctx);
    }
    // The elements from `node` up to the innermost paragraph or table, innermost first.
    let mut below: Vec<NodeId> = Vec::new();
    let mut cur = Some(node);
    while let Some(id) = cur {
        let n = tree.node_ref(id);
        if !n.is_element() {
            return None;
        }
        if is_word_ns(n.namespace()) && matches!(n.local_name(), "p" | "tbl" | "tr") {
            let ns = n.namespace().to_owned();
            let w = ns.as_str();
            let path: &[(&str, &str)] = match n.local_name() {
                "tr" => &[(w, "body"), (w, "tbl"), (w, "tr")],
                local => &[(w, "body"), (w, local)],
            };
            let mut ctx = Ctx::from_path(PartKind::Document, path)?;
            for &b in below.iter().rev() {
                let e = tree.node_ref(b);
                ctx = ctx.child((e.namespace(), e.local_name()))?;
            }
            return Some(ctx);
        }
        below.push(id);
        cur = n.parent().map(|p| p.id());
    }
    None
}

/// Puts the detached element `new` into `parent` where the schema says. A parent that is a property
/// container and out of order is rewritten in schema order. An element that cannot be added without a
/// violation, or that the content model does not name, is `invalid_payload`.
pub(super) fn place(tree: &mut SpanTree, parent: NodeId, new: NodeId) -> Result<(), StetError> {
    let (ns, local) = {
        let n = tree.node_ref(new);
        (n.namespace().to_owned(), n.local_name().to_owned())
    };
    let ns = &ns;
    // The properties of a paragraph, a run and a cell come first in it by the schema (CT_P, CT_R, CT_Tc),
    // whatever else it holds: an element of a namespace the schema does not know (or the order around it)
    // must not push them later, where a reader that looks for them first misses them.
    let parent_local = tree.node_ref(parent).local_name().to_owned();
    if is_word_ns(ns)
        && is_word_ns(tree.node_ref(parent).namespace())
        && matches!(
            (parent_local.as_str(), local.as_str()),
            ("p", "pPr") | ("r", "rPr") | ("tc", "tcPr")
        )
    {
        let first = tree.node_ref(parent).first_child().map(|c| c.id());
        return match first {
            Some(f) => insert(tree, parent, Some(f), new),
            None => tree.edit().append_child(parent, new),
        };
    }
    let ctx = ctx_of(tree, parent);
    let placement = insert_in_order(ctx, &*tree, parent, (ns, &local), Prefer::Last);
    match placement.fit {
        Fit::Valid | Fit::BestEffort | Fit::UnknownContext => {
            let kids = child_element_ids(tree, parent);
            match kids.get(placement.index) {
                Some(&before) => insert(tree, parent, Some(before), new),
                None => tree.edit().append_child(parent, new),
            }
        }
        Fit::Reorder(order) => reorder_into(tree, parent, Some(new), &order),
        Fit::NoValidSlot => Err(invalid(format!(
            "the property {local} cannot be added here: it is already there in another form, or it excludes a property that is"
        ))),
        Fit::NotAllowed => Err(invalid(format!(
            "the property {local} is not allowed in {}",
            tree.node_ref(parent).local_name()
        ))),
    }
}

/// Rewrites the element children of `parent` in `order`: `Some(i)` is the child that was at index `i`,
/// `None` is `new`.
fn reorder_into(
    tree: &mut SpanTree,
    parent: NodeId,
    new: Option<NodeId>,
    order: &[Option<usize>],
) -> Result<(), StetError> {
    let kids = child_element_ids(tree, parent);
    let sequence: Vec<NodeId> = order
        .iter()
        .filter_map(|e| match e {
            Some(i) => kids.get(*i).copied(),
            None => new,
        })
        .collect();
    for &k in &kids {
        detach(tree, k)?;
    }
    for k in sequence {
        tree.edit().append_child(parent, k)?;
    }
    Ok(())
}

/// After a removal from a property container: puts the children in schema order if they are not.
pub(super) fn restore_order(tree: &mut SpanTree, container: NodeId) -> Result<(), StetError> {
    let ctx = ctx_of(tree, container);
    if let Some(order) = reorder(ctx, &*tree, container, None) {
        let identity = order.iter().enumerate().all(|(i, e)| *e == Some(i));
        if !identity {
            reorder_into(tree, container, None, &order)?;
        }
    }
    Ok(())
}

/// True if the child element `n` has the expanded name (namespace URI, local name).
fn named(n: &NodeRef<'_>, ns: &str, local: &str) -> bool {
    n.is_element() && n.local_name() == local && n.namespace() == ns
}

/// Applies a property edit to a container: removes the children named in `clear`, replaces or adds the
/// children in `set`.
pub(super) fn apply_edit(
    tree: &mut SpanTree,
    container: NodeId,
    edit: &PropEdit,
) -> Result<(), StetError> {
    let doomed = |tree: &SpanTree, ns: &str, local: &str| -> Vec<NodeId> {
        let ns = super::names::in_part(tree, ns);
        tree.node_ref(container)
            .child_elements()
            .filter(|c| named(c, &ns, local))
            .map(|c| c.id())
            .collect()
    };
    let mut removed = false;
    for (ns, local) in &edit.clear {
        for id in doomed(tree, ns, local) {
            detach(tree, id)?;
            removed = true;
        }
    }
    if removed {
        restore_order(tree, container)?;
    }
    for el in &edit.set {
        let old = doomed(tree, &el.ns, &el.local);
        for id in &old {
            detach(tree, *id)?;
        }
        let new = super::tree_ops::build_element(tree, el)?;
        place(tree, container, new)?;
        if !old.is_empty() {
            restore_order(tree, container)?;
        }
    }
    Ok(())
}

/// The canonical form of an element: its name, its attributes in order of name, and its children.
pub(super) fn canon(n: &NodeRef<'_>) -> String {
    let mut attrs: Vec<String> = n
        .attrs()
        .filter(|a| !a.is_namespace_declaration())
        .map(|a| format!("{}|{}={}", a.namespace(), a.local_name(), a.value()))
        .collect();
    attrs.sort();
    let kids: Vec<String> = n.child_elements().map(|c| canon(&c)).collect();
    format!(
        "<{}|{} {} {}>",
        n.namespace(),
        n.local_name(),
        attrs.join(" "),
        kids.join("")
    )
}

/// True for a child of a container that a snapshot of it may hold: not a change-tracking element, and
/// one that the declaration of the snapshot allows.
pub(super) fn in_snapshot(c: &NodeRef<'_>, snap_ctx: Option<Ctx>) -> bool {
    if is_word_ns(c.namespace()) && is_change_tracking(c.local_name()) {
        return false;
    }
    snap_ctx.is_none_or(|x| x.allows_child((c.namespace(), c.local_name())))
}

/// The declaration of the snapshot inside the change element of a container: the container's own, one
/// level down through the change element.
pub(super) fn snapshot_ctx(tree: &SpanTree, container: NodeId, change_local: &str) -> Option<Ctx> {
    let n = tree.node_ref(container);
    let ctx = ctx_of(tree, container)?;
    let change_ns = n.namespace().to_owned();
    ctx.child((&change_ns, change_local))?
        .child((&change_ns, n.local_name()))
}

/// The canonical forms of what a container holds that a snapshot would hold, sorted.
pub(super) fn held(tree: &SpanTree, container: NodeId, change_local: &str) -> Vec<String> {
    let snap_ctx = snapshot_ctx(tree, container, change_local);
    let mut out: Vec<String> = tree
        .node_ref(container)
        .child_elements()
        .filter(|c| in_snapshot(c, snap_ctx))
        .map(|c| canon(&c))
        .collect();
    out.sort();
    out
}

/// The canonical forms of the children of a snapshot element, sorted.
pub(super) fn held_by_snapshot(tree: &SpanTree, snapshot: NodeId) -> Vec<String> {
    let mut out: Vec<String> = tree
        .node_ref(snapshot)
        .child_elements()
        .map(|c| canon(&c))
        .collect();
    out.sort();
    out
}

/// A detached snapshot of a container for its `*PrChange`: an element like the container, holding copies
/// of the children that the declaration of the snapshot allows, minus every change-tracking element of
/// the copy rule wherever it is in them.
pub(super) fn snapshot(
    tree: &mut SpanTree,
    container: NodeId,
    change_local: &str,
) -> Result<NodeId, StetError> {
    let snap_ctx = snapshot_ctx(tree, container, change_local);
    let (ns, local, kids) = {
        let n = tree.node_ref(container);
        let kids: Vec<NodeId> = n
            .child_elements()
            .filter(|c| in_snapshot(c, snap_ctx))
            .map(|c| c.id())
            .collect();
        (n.namespace().to_owned(), n.local_name().to_owned(), kids)
    };
    let snap = tree.edit().create_element(&ns, &local)?;
    for k in kids {
        let copy = tree.edit().clone_subtree(k)?;
        strip_deep(tree, copy)?;
        tree.edit().append_child(snap, copy)?;
    }
    // A container that Word or another tool wrote out of order is put in order where it changes; its
    // snapshot is a new element and is written in order too, or it would hold a violation of its own.
    if let Some(order) = reorder(snap_ctx, &*tree, snap, None)
        && !order.iter().enumerate().all(|(i, e)| *e == Some(i))
    {
        reorder_into(tree, snap, None, &order)?;
    }
    Ok(snap)
}

/// Removes the change-tracking elements below `node`, at any depth.
fn strip_deep(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    let mut doomed = Vec::new();
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        for c in tree.node_ref(id).child_elements() {
            if is_word_ns(c.namespace()) && is_change_tracking(c.local_name()) {
                doomed.push(c.id());
            } else {
                stack.push(c.id());
            }
        }
    }
    for id in doomed {
        detach(tree, id)?;
    }
    Ok(())
}

/// The element of `path` below `owner`, if all of it is there.
pub(super) fn find_chain(tree: &SpanTree, owner: NodeId, path: &[&str]) -> Option<NodeId> {
    let mut cur = owner;
    for name in path {
        cur = tree
            .node_ref(cur)
            .child_elements()
            .find(|c| is_w(c, name))?
            .id();
    }
    Some(cur)
}

/// The element of `path` below `owner`, made where the schema says if it is missing. Returns it and the
/// outermost element that was made, if any.
pub(super) fn ensure_chain(
    tree: &mut SpanTree,
    owner: NodeId,
    path: &[&str],
) -> Result<(NodeId, Option<NodeId>), StetError> {
    let wns_uri = wns(tree);
    let mut cur = owner;
    let mut made: Option<NodeId> = None;
    for name in path {
        let found = tree
            .node_ref(cur)
            .child_elements()
            .find(|c| is_w(c, name))
            .map(|c| c.id());
        cur = match found {
            Some(id) => id,
            None => {
                let el = create_w(tree, &wns_uri, name)?;
                place(tree, cur, el)?;
                made.get_or_insert(el);
                el
            }
        };
    }
    Ok((cur, made))
}
