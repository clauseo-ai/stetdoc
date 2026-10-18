//! The seam between `schema` and the XML tree.
//!
//! `schema` never names a concrete tree type. Everything it does with a document reads it through
//! [`Nodes`] (choosing where a child goes, the markup compatibility rules) or [`SchemaTree`]
//! (validating a part). The impls for the span tree live in `schema/span.rs`, over `crate::xml`,
//! because `xml` may depend only on `api`. The tests use the span tree, and a list of names for a
//! parent that has no other content. Names are always **expanded**: a namespace URI and a local name, never a prefix.

use std::borrow::Cow;

/// An expanded name: (namespace URI, local name). The URI is empty for no namespace.
pub type QName<'a> = (&'a str, &'a str);

/// One attribute as the validator needs to see it. `xmlns` declarations are not attributes.
#[derive(Clone, Debug)]
pub struct Attr<'a> {
    /// Namespace URI; empty for an unqualified attribute.
    pub namespace: &'a str,
    /// Local name.
    pub local: &'a str,
    /// The decoded value. It borrows when the text had no entity to decode.
    pub value: Cow<'a, str>,
}

/// Read-only navigation of elements: the name, parent, children, attributes and prefixes of one
/// element at a time. This is all that `insert_in_order`, `reorder` and the markup compatibility
/// rules (`mc:AlternateContent`, `mc:ProcessContent`) read, so they work on anything that can
/// answer these, including a bare `NodeRef` of the span tree (see `span::Refs`).
pub trait Nodes {
    /// A cheap handle to an element: an index, or a reference.
    type Node: Copy + PartialEq;

    /// The expanded name of an element.
    fn name(&self, node: Self::Node) -> QName<'_>;
    /// Element children, in document order. Comments and processing instructions are not
    /// children. The indices `insert_in_order` returns count these.
    fn children(&self, node: Self::Node) -> Vec<Self::Node>;
    /// The parent element, `None` for the root. `mc:Ignorable` and `mc:ProcessContent` apply to
    /// an element and everything below it, so the schema layer reads them from the ancestors.
    fn parent(&self, node: Self::Node) -> Option<Self::Node>;
    /// The attributes of an element, without namespace declarations.
    fn attributes(&self, node: Self::Node) -> Vec<Attr<'_>>;
    /// The namespace URI bound to `prefix` in the scope of `node` (used to read
    /// `mc:Ignorable`, `mc:ProcessContent` and `Requires`). An empty prefix is the default
    /// namespace.
    fn resolve_prefix(&self, node: Self::Node, prefix: &str) -> Option<String>;

    /// The first element child for which `pred` holds. The default collects the children; a tree
    /// that can walk them without doing so overrides it, which is what lets the rule that picks
    /// the branch of an `mc:AlternateContent` run without allocating.
    fn find_child(
        &self,
        node: Self::Node,
        pred: &mut dyn FnMut(Self::Node) -> bool,
    ) -> Option<Self::Node> {
        self.children(node).into_iter().find(|c| pred(*c))
    }

    /// The value of one attribute, by namespace URI (empty for none) and local name. The default
    /// reads `attributes`; a tree that can find one without collecting them overrides it.
    fn attribute(&self, node: Self::Node, namespace: &str, local: &str) -> Option<Cow<'_, str>> {
        self.attributes(node)
            .into_iter()
            .find(|a| a.namespace == namespace && a.local == local)
            .map(|a| a.value)
    }

    /// Whether the element has an attribute in the namespace. The markup compatibility rules ask it
    /// of every element they read, and almost none has one, so a tree that can answer without
    /// collecting the attributes overrides it.
    fn has_attribute_in(&self, node: Self::Node, namespace: &str) -> bool {
        self.attributes(node)
            .iter()
            .any(|a| a.namespace == namespace)
    }
}

/// A whole element tree, which `validate_part` walks: the navigation of [`Nodes`], the root, and
/// the test for character data.
pub trait SchemaTree: Nodes {
    /// The root element.
    fn root(&self) -> Self::Node;
    /// Whether the element directly contains character data other than whitespace. Whitespace
    /// is what `char::is_whitespace` accepts, which is wider than XML's four characters: the
    /// span tree decodes text and tests it that way, and the schema layer takes its word, so
    /// a no-break space between two elements is not reported as text.
    fn has_text(&self, node: Self::Node) -> bool;
}
