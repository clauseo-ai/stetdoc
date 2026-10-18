//! Names and small reads that every file of `revisions` shares: the `w` namespace of either dialect,
//! and the few things the compiler asks of a node. `wordml` has its own, private to it, and
//! `revisions` reads revisions only through `wordml`'s readers, so this holds names, not rules.

use crate::xml::{NodeId, NodeRef, SpanTree};
use std::borrow::Cow;

/// The `w` namespace, Transitional.
pub(super) const W_T: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// The `w` namespace, Strict.
pub(super) const W_S: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
/// The XML namespace, for `xml:space`.
pub(super) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
/// Office Math, Transitional.
pub(super) const M_T: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
/// Office Math, Strict.
pub(super) const M_S: &str = "http://purl.oclc.org/ooxml/officeDocument/math";

/// True for a namespace URI of the `w` vocabulary in either dialect.
pub(super) fn is_word_ns(uri: &str) -> bool {
    uri == W_T || uri == W_S
}

/// True if `n` is the element `local` of the `w` vocabulary.
pub(super) fn is_w(n: &NodeRef<'_>, local: &str) -> bool {
    n.is_element() && n.local_name() == local && is_word_ns(n.namespace())
}

/// True if `n` is an element of the Office Math vocabulary.
pub(super) fn is_math(n: &NodeRef<'_>) -> bool {
    n.is_element() && (n.namespace() == M_T || n.namespace() == M_S)
}

/// The decoded value of the `w:` attribute `local` of `n`.
pub(super) fn w_attr<'a>(n: &NodeRef<'a>, local: &str) -> Option<Cow<'a, str>> {
    n.attrs()
        .find(|a| a.local_name() == local && is_word_ns(a.namespace()))
        .map(|a| a.value())
}

/// The first child of `n` that is the element `local` of the `w` vocabulary.
pub(super) fn w_child<'a>(n: &NodeRef<'a>, local: &str) -> Option<NodeRef<'a>> {
    n.child_elements().find(|c| is_w(c, local))
}

/// The namespace URI that `w:` elements of this part use: the root's, which every story part (and the
/// styles and numbering parts) has in the `w` vocabulary. A part whose root is something else gets the
/// Transitional URI, which is what Word writes.
pub(super) fn wns(tree: &SpanTree) -> String {
    let uri = tree.root().namespace();
    if is_word_ns(uri) {
        uri.to_owned()
    } else {
        W_T.to_owned()
    }
}

/// A namespace that a caller names, as the part has it: the namespace of the `w` vocabulary, whichever of the
/// two a caller writes, is the one the part uses, and any other namespace is as given. A document in the
/// Strict namespaces gets Strict elements from a caller that knows only the Transitional URI.
pub(super) fn in_part(tree: &SpanTree, ns: &str) -> String {
    if is_word_ns(ns) {
        wns(tree)
    } else {
        ns.to_owned()
    }
}

/// The id of a revision element as a whole number.
pub(super) fn rev_id(n: &NodeRef<'_>) -> Option<i64> {
    w_attr(n, "id").and_then(|v| v.trim().parse::<i64>().ok())
}

/// True if a text node holds only XML white space, as the indentation between elements does.
pub(super) fn is_blank(n: &NodeRef<'_>) -> bool {
    !n.is_element()
        && n.text()
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
}

/// True if the first or last character of `text` is XML white space, so that a `w:t` or `w:delText`
/// holding it needs `xml:space="preserve"`.
pub(super) fn needs_preserve(text: &str) -> bool {
    let ws = |c: char| matches!(c, ' ' | '\t' | '\n' | '\r');
    text.chars().next().is_some_and(ws) || text.chars().next_back().is_some_and(ws)
}

/// The nearest ancestor of `node`, or `node` itself, that is an element named `local` of the `w`
/// vocabulary.
pub(super) fn ancestor_or_self_w<'a>(node: NodeRef<'a>, local: &str) -> Option<NodeRef<'a>> {
    let mut cur = Some(node);
    while let Some(n) = cur {
        if is_w(&n, local) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

/// True if `ancestor` is `node` or contains it.
pub(super) fn contains(tree: &SpanTree, ancestor: NodeId, node: NodeId) -> bool {
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if n.id() == ancestor {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// The next sibling that is not a blank text node.
pub(super) fn next_content<'a>(n: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    let mut cur = n.next_sibling();
    while let Some(c) = cur {
        if !is_blank(&c) {
            return Some(c);
        }
        cur = c.next_sibling();
    }
    None
}

/// The previous sibling that is not a blank text node.
pub(super) fn prev_content<'a>(n: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    let mut cur = n.prev_sibling();
    while let Some(c) = cur {
        if !is_blank(&c) {
            return Some(c);
        }
        cur = c.prev_sibling();
    }
    None
}

/// The first child that is not a blank text node.
pub(super) fn first_content<'a>(n: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    let first = n.first_child()?;
    if is_blank(&first) {
        next_content(&first)
    } else {
        Some(first)
    }
}
