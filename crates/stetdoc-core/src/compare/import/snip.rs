//! A subtree of one package as plain data, so that it can be changed and written into another package's tree (design
//! `compare.md`, 7.1).
//!
//! `xml` copies within a tree. Across two trees the copy is made with the editor's own calls (`create_element`, `set_attr`,
//! `create_text`, `append_child`), and between the read and the write the importer rewrites what names something of the source
//! package: style ids, list ids, relationship ids, note ids. Nothing here knows about any of that: a [`Snip`] is an element, its
//! attributes and its children, with the id the element had in the source so that a range of a paragraph can say which nodes it
//! covers.

use crate::api::error::{ErrorCode, StetError};
use crate::revisions::import_writer::Detached;
use crate::xml::{NodeId, NodeKind, NodeRef, SpanTree};
use std::collections::BTreeMap;

/// The `w` namespace in its Transitional form.
pub(super) const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// The `w` namespace in its Strict form.
pub(super) const W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
/// The relationships namespace, whose attributes name relationships.
pub(super) const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Office Math, in both dialects.
pub(super) const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
/// Office Math, Strict.
pub(super) const M_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/math";
/// Markup compatibility.
pub(super) const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
/// The namespace of `xml:space`.
pub(super) const XML: &str = "http://www.w3.org/XML/1998/namespace";
/// The deepest nesting that is copied. Content deeper than this is dropped, so that a hostile document cannot make the
/// copy recurse without bound.
const MAX_DEPTH: usize = 128;

/// One attribute: namespace URI (empty for none), local name, decoded value.
pub(super) type Attr = (String, String, String);

/// An element and what is below it.
#[derive(Clone, Debug)]
pub(super) struct Snip {
    /// The node in the source tree.
    pub src: NodeId,
    /// Namespace URI.
    pub ns: String,
    /// Local name.
    pub local: String,
    /// The attributes, namespace declarations left out.
    pub attrs: Vec<Attr>,
    /// The children, in order.
    pub kids: Vec<Kid>,
}

/// A child of a [`Snip`].
#[derive(Clone, Debug)]
pub(super) enum Kid {
    /// An element.
    El(Snip),
    /// Text, with the text node it came from.
    Text(NodeId, String),
}

impl Snip {
    /// True for the element `local` of the `w` vocabulary.
    pub(super) fn is_w(&self, local: &str) -> bool {
        self.local == local && (self.ns == W || self.ns == W_STRICT)
    }

    /// The value of the `w:` attribute `local`.
    pub(super) fn w_attr(&self, local: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(ns, l, _)| l == local && (ns == W || ns == W_STRICT))
            .map(|(_, _, v)| v.as_str())
    }

    /// Sets (or adds) the `w:` attribute `local`.
    pub(super) fn set_w_attr(&mut self, local: &str, value: &str) {
        match self
            .attrs
            .iter_mut()
            .find(|(ns, l, _)| l == local && (*ns == W || *ns == W_STRICT))
        {
            Some(a) => a.2 = value.to_owned(),
            None => self
                .attrs
                .push((W.to_owned(), local.to_owned(), value.to_owned())),
        }
    }

    /// The first child element that is `w:local`.
    pub(super) fn w_child(&self, local: &str) -> Option<&Snip> {
        self.kids.iter().find_map(|k| match k {
            Kid::El(e) if e.is_w(local) => Some(e),
            _ => None,
        })
    }

    /// Every element below this one, itself included, in document order.
    pub(super) fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Snip)) {
        f(self);
        for k in &self.kids {
            if let Kid::El(e) = k {
                e.walk(f);
            }
        }
    }
}

/// The namespaces that a snip uses besides `w` and `r`: URI to the prefix the source gave it and whether the source's root marks
/// that prefix as ignorable. They are bound on the target's root when the snip is written.
#[derive(Clone, Debug, Default)]
pub(super) struct Used(pub BTreeMap<String, (String, bool)>);

/// What a read of a source tree needs to know: which prefixes the source's root marks ignorable.
pub(super) struct Reader<'a> {
    tree: &'a SpanTree,
    ignorable: Vec<String>,
    /// The foreign namespaces met.
    pub used: Used,
    /// The `mc:Requires` and `mc:Ignorable` prefixes of elements read, resolved to URIs at the element.
    pub requires: BTreeMap<String, String>,
}

impl<'a> Reader<'a> {
    /// A reader of a source tree.
    pub(super) fn new(tree: &'a SpanTree) -> Reader<'a> {
        let ignorable = tree
            .root()
            .attr(MC, "Ignorable")
            .map(|v| v.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default();
        Reader {
            tree,
            ignorable,
            used: Used::default(),
            requires: BTreeMap::new(),
        }
    }

    /// Reads the subtree at `id`. `None` when it is not an element.
    pub(super) fn read(&mut self, id: NodeId) -> Option<Snip> {
        let n = self.tree.node_ref(id);
        if !n.is_element() {
            return None;
        }
        Some(self.element(&n, 0))
    }

    fn note_ns(&mut self, ns: &str, prefix: &str) {
        if ns.is_empty() || ns == W || ns == W_STRICT || ns == R || ns == XML || ns == MC {
            return;
        }
        let ignorable = self.ignorable.iter().any(|p| p == prefix);
        self.used
            .0
            .entry(ns.to_owned())
            .or_insert((prefix.to_owned(), ignorable));
    }

    fn element(&mut self, n: &NodeRef<'_>, depth: usize) -> Snip {
        self.note_ns(n.namespace(), n.prefix());
        let mut attrs = Vec::new();
        for a in n.attrs() {
            if a.is_namespace_declaration() {
                continue;
            }
            let name = a.name();
            let prefix = name.split_once(':').map_or("", |(p, _)| p);
            self.note_ns(a.namespace(), prefix);
            if a.namespace() == MC
                && (a.local_name() == "Requires" || a.local_name() == "Ignorable")
            {
                for token in a.value().split_whitespace() {
                    if let Some(uri) = n.namespace_of_prefix(token) {
                        self.requires.insert(token.to_owned(), uri);
                    }
                }
            }
            attrs.push((
                a.namespace().to_owned(),
                a.local_name().to_owned(),
                a.value().into_owned(),
            ));
        }
        // Text that keeps its white space by an ancestor's `xml:space` (a Word 2007 file says it once, on `w:document`) says it
        // itself in the copy, which lands under another root.
        let text = matches!(n.local_name(), "t" | "delText")
            && (n.namespace() == W || n.namespace() == W_STRICT);
        if text
            && !attrs.iter().any(|(ns, l, _)| ns == XML && l == "space")
            && crate::wordml::space_preserved(n)
        {
            attrs.push((XML.to_owned(), "space".to_owned(), "preserve".to_owned()));
        }
        let mut kids = Vec::new();
        if depth < MAX_DEPTH {
            for c in n.children() {
                match c.kind() {
                    NodeKind::Element => kids.push(Kid::El(self.element(&c, depth + 1))),
                    NodeKind::Text | NodeKind::CData => {
                        kids.push(Kid::Text(c.id(), c.text().into_owned()));
                    }
                    _ => {}
                }
            }
        }
        Snip {
            src: n.id(),
            ns: n.namespace().to_owned(),
            local: n.local_name().to_owned(),
            attrs,
            kids,
        }
    }
}

fn internal(message: &str) -> StetError {
    StetError::new(ErrorCode::Internal, message)
}

/// Writes a snip into `tree` as detached nodes and returns the top element. `w_ns` is the `w` namespace that `tree` uses: an
/// element or attribute of either `w` dialect is written in it.
pub(super) fn write(
    tree: &mut Detached<'_>,
    snip: &Snip,
    w_ns: &str,
    requires: &BTreeMap<String, String>,
) -> Result<NodeId, StetError> {
    let ed = tree;
    let map_ns = |ns: &str| -> String {
        if ns == W || ns == W_STRICT {
            w_ns.to_owned()
        } else {
            ns.to_owned()
        }
    };
    let top = ed.create_element(&map_ns(&snip.ns), &snip.local)?;
    // a stack of (source element, new element, next child to write)
    let mut stack: Vec<(&Snip, NodeId, usize)> = vec![(snip, top, 0)];
    let mut first = true;
    while let Some((s, id, at)) = stack.pop() {
        if at == 0 {
            if !first {
                // the element itself was created by its parent, attributes still to set
            }
            first = false;
            for (ns, local, value) in &s.attrs {
                let value = if ns == MC && (local == "Requires" || local == "Ignorable") {
                    rewrite_prefixes(ed, value, requires)?
                } else {
                    value.clone()
                };
                ed.set_attr(id, &map_ns(ns), local, &value)?;
            }
        }
        match s.kids.get(at) {
            None => {}
            Some(Kid::Text(_, text)) => {
                let t = ed.create_text(text)?;
                ed.append_child(id, t)?;
                stack.push((s, id, at + 1));
            }
            Some(Kid::El(e)) => {
                let c = ed.create_element(&map_ns(&e.ns), &e.local)?;
                ed.append_child(id, c)?;
                stack.push((s, id, at + 1));
                stack.push((e, c, 0));
            }
        }
    }
    Ok(top)
}

/// The prefixes of an `mc:Requires` or `mc:Ignorable` value, bound on the target's root to the namespace they meant in the
/// source: the prefix the root gives that namespace, which may not be the one the source used.
fn rewrite_prefixes(
    ed: &mut Detached<'_>,
    value: &str,
    requires: &BTreeMap<String, String>,
) -> Result<String, StetError> {
    let mut out: Vec<String> = Vec::new();
    for token in value.split_whitespace() {
        match requires.get(token) {
            Some(uri) => out.push(ed.bind_namespace(uri, token, false)?),
            None => out.push(token.to_owned()),
        }
    }
    Ok(out.join(" "))
}

/// Binds the namespaces that a snip uses on the target's root, so that its elements and attributes are written under the prefix
/// the source gave them where the target allows it, and are ignorable where the source says so.
pub(super) fn bind_used(ed: &mut Detached<'_>, used: &Used) -> Result<(), StetError> {
    for (uri, (prefix, ignorable)) in &used.0 {
        ed.bind_namespace(uri, prefix, *ignorable)
            .map_err(|e| internal(&format!("a namespace could not be bound: {e}")))?;
    }
    Ok(())
}
