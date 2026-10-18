//! Tests for the schema module, at its interface: `insert_in_order`, `reorder`,
//! `validate_part` and the violation set. They parse XML into the span tree, which is the
//! [`SchemaTree`] the engine reads, and `ListTree` stands in for a parent whose children are only
//! names. `order` holds the tests of insertion, `validate` those of the validator, `scale` the
//! time bounds, `mc` the markup compatibility rules.

mod ctx;
mod mc;
mod order;
mod scale;
mod span;
mod validate;
pub(in crate::schema) mod walk;

pub(super) use mc::Gen;

use super::*;
use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeId, SpanTree};

pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
pub const NS_DECLS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:x="urn:example:unknown""#;

/// A parent with the given children and nothing else. Node 0 is the parent, node `i + 1` is
/// child `i`. For tests that care about names and order only.
pub(super) struct ListTree {
    names: Vec<(String, String)>,
}

impl ListTree {
    pub(super) fn new(children: &[QName<'_>]) -> ListTree {
        ListTree {
            names: children
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    }
}

impl Nodes for ListTree {
    type Node = usize;

    fn name(&self, n: usize) -> QName<'_> {
        match n.checked_sub(1).and_then(|i| self.names.get(i)) {
            Some((a, b)) => (a.as_str(), b.as_str()),
            None => ("", "parent"),
        }
    }
    fn children(&self, n: usize) -> Vec<usize> {
        if n == 0 {
            (1..=self.names.len()).collect()
        } else {
            Vec::new()
        }
    }
    fn parent(&self, n: usize) -> Option<usize> {
        (n != 0).then_some(0)
    }
    fn attributes(&self, _: usize) -> Vec<Attr<'_>> {
        Vec::new()
    }
    fn resolve_prefix(&self, _: usize, _: &str) -> Option<String> {
        None
    }
}

/// Parses `xml` into a span tree.
pub(super) fn parse(xml: &str) -> SpanTree {
    SpanTree::parse("test.xml", Bytes::from(xml.as_bytes()), &Limits::default())
        .expect("test xml parses")
}

pub(super) fn violations(kind: PartKind, xml: &str) -> ViolationSet {
    validate_part(kind, &parse(xml)).set
}

pub(super) fn doc_xml(body: &str) -> String {
    format!(r#"<w:document {NS_DECLS} mc:Ignorable="w14"><w:body>{body}</w:body></w:document>"#)
}

/// What the validator reports, as a person reads it: the kind of each violation (a surplus
/// element is `TooMany` here, and `OutOfOrder` in the key) and its subject.
pub(super) fn reported(kind: PartKind, xml: &str) -> Vec<(ViolationKind, String)> {
    let report = validate_part(kind, &parse(xml));
    report
        .violations
        .iter()
        .map(|v| (v.kind, v.key.subject.clone()))
        .collect()
}

pub(super) fn kinds(set: &ViolationSet) -> Vec<(ViolationKind, String)> {
    set.iter()
        .map(|(k, _)| (k.kind, k.subject.clone()))
        .collect()
}

pub(super) fn w(local: &str) -> QName<'_> {
    (W, local)
}

/// Where `insert_in_order` puts `new` among the given children of a parent with context `ctx`.
pub(super) fn place(
    ctx: Option<Ctx>,
    siblings: &[QName<'_>],
    new: QName<'_>,
    prefer: Prefer,
) -> Placement {
    insert_in_order(ctx, &ListTree::new(siblings), 0, new, prefer)
}

/// Parses `xml`, finds the first element with the local name `parent`, works out its context
/// from its ancestors and inserts `new` into it.
pub(super) fn place_in_xml(
    part: PartKind,
    xml: &str,
    parent: &str,
    new: QName<'_>,
    prefer: Prefer,
) -> Placement {
    let tree = parse(xml);
    let node = nth(&tree, parent, 0);
    let ctx = ctx_at(part, &tree, node);
    insert_in_order(ctx, &tree, node, new, prefer)
}

/// The elements of a tree in document order.
pub(super) fn elements(tree: &SpanTree) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().id()];
    while let Some(id) = stack.pop() {
        out.push(id);
        let mut kids = tree.children(id);
        kids.reverse();
        stack.extend(kids);
    }
    out
}

/// The `n`th element (in document order, from 0) with the local name `local`.
pub(super) fn nth(tree: &SpanTree, local: &str, n: usize) -> NodeId {
    elements(tree)
        .into_iter()
        .filter(|e| tree.name(*e).1 == local)
        .nth(n)
        .expect("the element is in the document")
}
