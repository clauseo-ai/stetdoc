//! The span tree as a schema tree.
//!
//! `SpanTree` is a [`SchemaTree`] whose handles are `NodeId`s, for code that holds the tree (the
//! check, the edit pipeline). [`Refs`] is [`Nodes`] over `NodeRef`s: a `NodeRef` carries its
//! tree, so every question about an element can be answered from the handle, and code that holds
//! only `NodeRef`s (`wordml` reading a paragraph) can call the rules of this module without the
//! tree. Both read through the same functions below, so they answer alike.
//!
//! The impls live here because `xml` may depend only on `api`, and `schema` may depend on `xml`.
//! `xml` provides the read surface and nothing in it knows about the schema.

use super::tree::{Attr, Nodes, QName, SchemaTree};
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::borrow::Cow;
use std::marker::PhantomData;

/// The expanded name of an element: its namespace URI and local name, never its prefix.
fn name_of<'a>(n: NodeRef<'a>) -> QName<'a> {
    (n.namespace(), n.local_name())
}

/// The parent element. The root element's parent is the document node, which is not an element.
fn parent_of<'a>(n: NodeRef<'a>) -> Option<NodeRef<'a>> {
    n.parent().filter(NodeRef::is_element)
}

/// The attributes that are data: the namespace declarations are not.
fn attributes_of<'a>(n: NodeRef<'a>) -> Vec<Attr<'a>> {
    n.attrs()
        .filter(|a| !a.is_namespace_declaration())
        .map(|a| Attr {
            namespace: a.namespace(),
            local: a.local_name(),
            value: a.value(),
        })
        .collect()
}

/// Whether the element has an attribute that is data in the namespace.
fn has_attribute_in_ns(n: NodeRef<'_>, namespace: &str) -> bool {
    n.attrs()
        .any(|a| !a.is_namespace_declaration() && a.namespace() == namespace)
}

/// One attribute that is data, by namespace and local name, without collecting the others.
fn attribute_of<'a>(n: NodeRef<'a>, namespace: &str, local: &str) -> Option<Cow<'a, str>> {
    n.attrs()
        .find(|a| {
            !a.is_namespace_declaration() && a.namespace() == namespace && a.local_name() == local
        })
        .map(|a| a.value())
}

impl Nodes for SpanTree {
    type Node = NodeId;

    fn name(&self, node: NodeId) -> QName<'_> {
        name_of(self.node_ref(node))
    }

    fn children(&self, node: NodeId) -> Vec<NodeId> {
        self.node_ref(node)
            .child_elements()
            .map(|c| c.id())
            .collect()
    }

    fn parent(&self, node: NodeId) -> Option<NodeId> {
        parent_of(self.node_ref(node)).map(|p| p.id())
    }

    fn attributes(&self, node: NodeId) -> Vec<Attr<'_>> {
        attributes_of(self.node_ref(node))
    }

    fn resolve_prefix(&self, node: NodeId, prefix: &str) -> Option<String> {
        self.node_ref(node).namespace_of_prefix(prefix)
    }

    fn find_child(&self, node: NodeId, pred: &mut dyn FnMut(NodeId) -> bool) -> Option<NodeId> {
        self.node_ref(node)
            .child_elements()
            .map(|c| c.id())
            .find(|c| pred(*c))
    }

    fn attribute(&self, node: NodeId, namespace: &str, local: &str) -> Option<Cow<'_, str>> {
        attribute_of(self.node_ref(node), namespace, local)
    }

    fn has_attribute_in(&self, node: NodeId, namespace: &str) -> bool {
        has_attribute_in_ns(self.node_ref(node), namespace)
    }
}

impl SchemaTree for SpanTree {
    fn root(&self) -> NodeId {
        SpanTree::root(self).id()
    }

    fn has_text(&self, node: NodeId) -> bool {
        self.node_ref(node).has_text()
    }
}

/// [`Nodes`] over `NodeRef`s. It holds nothing: the handles carry the tree.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Refs<'a>(PhantomData<&'a ()>);

impl Refs<'_> {
    pub(crate) fn new() -> Self {
        Refs(PhantomData)
    }
}

impl<'a> Nodes for Refs<'a> {
    type Node = NodeRef<'a>;

    fn name(&self, node: NodeRef<'a>) -> QName<'_> {
        name_of(node)
    }

    fn children(&self, node: NodeRef<'a>) -> Vec<NodeRef<'a>> {
        node.child_elements().collect()
    }

    fn parent(&self, node: NodeRef<'a>) -> Option<NodeRef<'a>> {
        parent_of(node)
    }

    fn attributes(&self, node: NodeRef<'a>) -> Vec<Attr<'_>> {
        attributes_of(node)
    }

    fn resolve_prefix(&self, node: NodeRef<'a>, prefix: &str) -> Option<String> {
        node.namespace_of_prefix(prefix)
    }

    fn find_child(
        &self,
        node: NodeRef<'a>,
        pred: &mut dyn FnMut(NodeRef<'a>) -> bool,
    ) -> Option<NodeRef<'a>> {
        node.child_elements().find(|c| pred(*c))
    }

    fn attribute(&self, node: NodeRef<'a>, namespace: &str, local: &str) -> Option<Cow<'_, str>> {
        attribute_of(node, namespace, local)
    }

    fn has_attribute_in(&self, node: NodeRef<'a>, namespace: &str) -> bool {
        has_attribute_in_ns(node, namespace)
    }
}
