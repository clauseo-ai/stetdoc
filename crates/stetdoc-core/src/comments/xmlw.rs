//! Small builders over the `xml` editor: an element with its attributes and children, and text.

use crate::api::error::StetError;
use crate::schema::{Fit, PartKind, Prefer, ctx_at, insert_in_order};
use crate::xml::{NodeId, SpanTree};

/// The `xml` namespace, for `xml:space`.
pub(super) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// An attribute: the namespace URI (empty for none), the local name and the value.
pub(super) type Attr<'a> = (&'a str, &'a str, &'a str);

/// A new detached element with these attributes and children. Its prefixes are chosen when it is inserted.
pub(super) fn el(
    tree: &mut SpanTree,
    ns: &str,
    local: &str,
    attrs: &[Attr<'_>],
    kids: &[NodeId],
) -> Result<NodeId, StetError> {
    let mut ed = tree.edit();
    let e = ed.create_element(ns, local)?;
    for (ans, alocal, value) in attrs {
        ed.set_attr(e, ans, alocal, value)?;
    }
    for kid in kids {
        ed.append_child(e, *kid)?;
    }
    Ok(e)
}

/// A new detached `w:t` or the like: an element holding one text node.
pub(super) fn text_el(
    tree: &mut SpanTree,
    ns: &str,
    local: &str,
    attrs: &[Attr<'_>],
    text: &str,
) -> Result<NodeId, StetError> {
    let mut ed = tree.edit();
    let e = ed.create_element(ns, local)?;
    for (ans, alocal, value) in attrs {
        ed.set_attr(e, ans, alocal, value)?;
    }
    let t = ed.create_text(text)?;
    ed.append_child(e, t)?;
    Ok(e)
}

/// Appends `child` as the last child of `parent`.
pub(super) fn append(tree: &mut SpanTree, parent: NodeId, child: NodeId) -> Result<(), StetError> {
    tree.edit().append_child(parent, child)
}

/// Removes `node` from its parent.
pub(super) fn detach(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    tree.edit().detach(node)
}

/// Sets an attribute.
pub(super) fn set(
    tree: &mut SpanTree,
    el: NodeId,
    ns: &str,
    local: &str,
    value: &str,
) -> Result<(), StetError> {
    tree.edit().set_attr(el, ns, local, value)
}

/// Puts the detached element `child` into `parent` where the schema says (`schema::insert_in_order`): for the
/// lists written here, a `w:style` in `w:styles` and a `w:comment` in `w:comments`, that is after the last of
/// its kind. A parent that the schema cannot place it in is not rewritten: the element goes last.
pub(super) fn place(
    tree: &mut SpanTree,
    kind: PartKind,
    parent: NodeId,
    child: NodeId,
) -> Result<(), StetError> {
    let (ns, local) = {
        let n = tree.node_ref(child);
        (n.namespace().to_owned(), n.local_name().to_owned())
    };
    let ctx = ctx_at(kind, &*tree, parent);
    let placement = insert_in_order(ctx, &*tree, parent, (&ns, &local), Prefer::Last);
    let before = match placement.fit {
        Fit::Valid | Fit::BestEffort | Fit::UnknownContext => tree
            .node_ref(parent)
            .child_elements()
            .nth(placement.index)
            .map(|n| n.id()),
        _ => None,
    };
    tree.edit().insert_before(parent, before, child)
}
