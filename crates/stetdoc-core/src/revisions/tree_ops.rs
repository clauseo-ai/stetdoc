//! Small edits of a span tree that the compiler builds everything from: making an element with
//! attributes, moving a node, renaming an element and copying attributes. Each takes the tree itself,
//! and none takes an id from the transaction, so the caller fetches ids first.

use super::errors::internal;
use super::names::{XML_NS, is_w, needs_preserve};
use crate::api::error::StetError;
use crate::xml::{NodeId, NodeKind, NodeRef, SpanTree};

/// A new detached element `local` in the `w` namespace `wns`.
pub(super) fn create_w(tree: &mut SpanTree, wns: &str, local: &str) -> Result<NodeId, StetError> {
    tree.edit().create_element(wns, local)
}

/// Sets the `w:` attribute `local` of `el`.
pub(super) fn set_w_attr(
    tree: &mut SpanTree,
    el: NodeId,
    wns: &str,
    local: &str,
    value: &str,
) -> Result<(), StetError> {
    tree.edit().set_attr(el, wns, local, value)
}

/// Inserts `child` as a child of `parent`, before `before` or last.
pub(super) fn insert(
    tree: &mut SpanTree,
    parent: NodeId,
    before: Option<NodeId>,
    child: NodeId,
) -> Result<(), StetError> {
    tree.edit().insert_before(parent, before, child)
}

/// Moves `node` to be a child of `parent`, before `before` or last. The node keeps its id and
/// everything under it.
pub(super) fn move_to(
    tree: &mut SpanTree,
    node: NodeId,
    parent: NodeId,
    before: Option<NodeId>,
) -> Result<(), StetError> {
    let mut ed = tree.edit();
    ed.detach(node)?;
    ed.insert_before(parent, before, node)
}

/// Removes `node` from its parent. The node keeps its id and everything under it, in the arena.
pub(super) fn detach(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    tree.edit().detach(node)
}

/// The ids of the children of `node`, in order.
pub(super) fn child_ids(tree: &SpanTree, node: NodeId) -> Vec<NodeId> {
    tree.node_ref(node).children().map(|c| c.id()).collect()
}

/// The ids of the child elements of `node`, in order.
pub(super) fn child_element_ids(tree: &SpanTree, node: NodeId) -> Vec<NodeId> {
    tree.node_ref(node)
        .child_elements()
        .map(|c| c.id())
        .collect()
}

/// The parent of `node`, if it has one.
pub(super) fn parent_of(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    tree.node_ref(node).parent().map(|p| p.id())
}

/// The next sibling of `node`, if there is one.
pub(super) fn next_sibling_of(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    tree.node_ref(node).next_sibling().map(|n| n.id())
}

/// The attributes of an element as owned values, without namespace declarations: the namespace URI
/// (empty for none), the local name and the decoded value.
pub(super) fn attrs_of(n: &NodeRef<'_>) -> Vec<(String, String, String)> {
    n.attrs()
        .filter(|a| !a.is_namespace_declaration())
        .map(|a| {
            (
                a.namespace().to_owned(),
                a.local_name().to_owned(),
                a.value().into_owned(),
            )
        })
        .collect()
}

/// Gives `to` the attributes of `from`, in order.
pub(super) fn copy_attrs(tree: &mut SpanTree, from: NodeId, to: NodeId) -> Result<(), StetError> {
    let attrs = attrs_of(&tree.node_ref(from));
    let mut ed = tree.edit();
    for (ns, local, value) in attrs {
        ed.set_attr(to, &ns, &local, &value)?;
    }
    Ok(())
}

/// Replaces the element `el` by a new element `local` in the same namespace, with the same attributes
/// and the same children (moved, so text nodes keep their ids), at the same place. Returns the new
/// element.
pub(super) fn rename(tree: &mut SpanTree, el: NodeId, local: &str) -> Result<NodeId, StetError> {
    let (ns, parent, next) = {
        let n = tree.node_ref(el);
        let parent = n
            .parent()
            .ok_or_else(|| internal("an element to rename has no parent"))?
            .id();
        (
            n.namespace().to_owned(),
            parent,
            n.next_sibling().map(|s| s.id()),
        )
    };
    let new = tree.edit().create_element(&ns, local)?;
    copy_attrs(tree, el, new)?;
    for child in child_ids(tree, el) {
        move_to(tree, child, new, None)?;
    }
    insert(tree, parent, next, new)?;
    detach(tree, el)?;
    Ok(new)
}

/// Makes sure a `w:t` or `w:delText` that starts or ends with white space keeps it: it says `xml:space="preserve"`,
/// unless an ancestor already does.
pub(super) fn ensure_preserve(tree: &mut SpanTree, el: NodeId) -> Result<(), StetError> {
    // `preserve` on an ancestor counts, as XML inherits it (`says_preserve`), so it is not said twice.
    let (needs, has) = {
        let n = tree.node_ref(el);
        (needs_preserve(&n.text_content()), says_preserve(tree, el))
    };
    if needs && !has {
        tree.edit().set_attr(el, XML_NS, "space", "preserve")?;
    }
    Ok(())
}

/// Whether a `w:t` or `w:delText` keeps its white space by its own `xml:space` or an ancestor's, as XML inherits it.
pub(super) fn says_preserve(tree: &SpanTree, el: NodeId) -> bool {
    crate::wordml::space_preserved(&tree.node_ref(el))
}

/// Removes every change-tracking child of `container` (a copy of a `w:rPr` or `w:pPr`): the copy rule.
pub(super) fn strip_change_tracking(
    tree: &mut SpanTree,
    container: NodeId,
) -> Result<(), StetError> {
    let doomed: Vec<NodeId> = tree
        .node_ref(container)
        .child_elements()
        .filter(|c| {
            super::names::is_word_ns(c.namespace())
                && super::policy::is_change_tracking(c.local_name())
        })
        .map(|c| c.id())
        .collect();
    for id in doomed {
        detach(tree, id)?;
    }
    Ok(())
}

/// True if the node is a text or CDATA node.
pub(super) fn is_text_node(n: &NodeRef<'_>) -> bool {
    matches!(n.kind(), NodeKind::Text | NodeKind::CData)
}

/// The `w:rPr` child of a run or the like.
pub(super) fn rpr_of(tree: &SpanTree, run: NodeId) -> Option<NodeId> {
    tree.node_ref(run)
        .child_elements()
        .find(|c| is_w(c, "rPr"))
        .map(|c| c.id())
}

/// Builds a detached element from its description, with its attributes and children. Prefixes are
/// given to the element when it is inserted, so a namespace that the part does not declare yet is
/// declared on the root then.
pub(super) fn build_element(
    tree: &mut SpanTree,
    spec: &super::types::PropElement,
) -> Result<NodeId, StetError> {
    let uri = super::names::in_part(tree, &spec.ns);
    let el = tree.edit().create_element(&uri, &spec.local)?;
    for (ns, local, value) in &spec.attrs {
        let ns = super::names::in_part(tree, ns);
        tree.edit().set_attr(el, &ns, local, value)?;
    }
    for child in &spec.children {
        let c = build_element(tree, child)?;
        tree.edit().append_child(el, c)?;
    }
    Ok(el)
}
