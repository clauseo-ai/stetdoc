//! Editing a span tree. Every method marks exactly what it changes as dirty.
//!
//! See `docs/design/package-and-xml.md` sections 2.3 and 2.5. Nothing here re-serializes anything: an edit changes
//! spans and flags, and `serialize` does the writing. Namespaces are in `ns.rs` and `late.rs`.

use super::NodeId;
use super::errors::{invalid, limit, not_found};
use super::text::{
    escape_attr, escape_text, is_ncname, join_bytes, raw_range_for_chars, whitespace_only,
};
use super::tree::{
    Attr, F_CHILDREN_DIRTY, F_LATE_NAME, F_NEW, F_SELF_CLOSING, F_SUBTREE_DIRTY, F_TAG_DIRTY, NONE,
    Node, NodeKind, Span, SpanTree, XMLNS_NS,
};
use crate::api::error::StetError;

/// The markup-compatibility namespace, which holds `mc:Ignorable`.
pub const MC_NS: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

/// A mutable view of a tree. Created by `SpanTree::edit`.
pub struct Editor<'a> {
    pub(super) tree: &'a mut SpanTree,
}

impl SpanTree {
    /// Starts editing. Every change made through the `Editor` marks exactly what it touches as dirty.
    pub fn edit(&mut self) -> Editor<'_> {
        Editor { tree: self }
    }
}

pub(super) fn no_node() -> StetError {
    not_found("no such node in this tree")
}

impl Editor<'_> {
    pub(super) fn node(&self, id: NodeId) -> Result<Node, StetError> {
        self.tree
            .nodes
            .get(id.0 as usize)
            .copied()
            .ok_or_else(no_node)
    }

    pub(super) fn element(&self, id: NodeId) -> Result<Node, StetError> {
        let n = self.node(id)?;
        if n.kind == NodeKind::Element {
            Ok(n)
        } else {
            Err(invalid("this operation needs an element"))
        }
    }

    /// Marks `id` and every ancestor as containing a change. Stops at one already marked. Every
    /// edit that changes the tree comes through here or through `push`, which is where the edit
    /// count (`SpanTree::edit_count`) moves. A node is read before it is changed, so that the chunk
    /// it is in is copied only when it has to be: a clone shares the chunks, and a node that is
    /// already marked changes nothing.
    pub(super) fn touch(&mut self, mut id: u32) {
        self.tree.edits += 1;
        while id != NONE {
            let Some(n) = self.tree.nodes.get(id as usize) else {
                return;
            };
            if n.flags & F_SUBTREE_DIRTY != 0 {
                return;
            }
            let parent = n.parent;
            if let Some(n) = self.tree.node_mut(id) {
                n.flags |= F_SUBTREE_DIRTY;
            }
            id = parent;
        }
    }

    pub(super) fn flag(&mut self, id: u32, flags: u8) {
        if self.tree.node(id).flags & flags != flags
            && let Some(n) = self.tree.node_mut(id)
        {
            n.flags |= flags;
        }
        self.touch(id);
    }

    // ---- attributes ------------------------------------------------------------------

    /// Gives an element a new list of attributes. The list goes at the end of the arena, or takes
    /// the place of the element's old list when that is already last, so changing one element over
    /// and over does not grow the arena.
    pub(super) fn replace_attrs(&mut self, el: u32, list: Vec<Attr>) -> Result<(), StetError> {
        if list.len() > self.tree.max_attrs {
            return Err(limit(format!(
                "an element would have more than {} attributes",
                self.tree.max_attrs
            )));
        }
        let old = *self.tree.node(el);
        if old.attr_count > 0
            && old.attr_start as usize + usize::from(old.attr_count) == self.tree.attrs.end()
        {
            self.tree.attrs.truncate_last(old.attr_start as usize);
        }
        let count = u16::try_from(list.len()).map_err(|_| limit("too many XML attributes"))?;
        let start = self
            .tree
            .attrs
            .push_slice(&list)
            .and_then(|at| u32::try_from(at).ok())
            .ok_or_else(|| limit("too many XML attributes"))?;
        if let Some(n) = self.tree.node_mut(el) {
            n.attr_start = start;
            n.attr_count = count;
        }
        Ok(())
    }

    pub(super) fn find_attr(&self, n: &Node, ns: &str, local: &str) -> Option<usize> {
        self.tree.node_attrs(n).iter().position(|a| {
            let name = self.tree.str_of(a.name);
            let l = match a.written_prefix_len() {
                0 => name,
                p => name.get(p + 1..).unwrap_or(""),
            };
            l == local && self.tree.uri(a.ns) == ns
        })
    }

    /// Sets an attribute, creating it if needed. `ns` is the namespace URI, empty for none.
    ///
    /// A changed attribute keeps its name, spacing and quote style. A new attribute is added after
    /// the existing ones. On an element in the tree its prefix is one that is in scope at the
    /// element. If the namespace has no prefix there, it is declared on the root, or on the element
    /// itself when a closer declaration hides the root's. On an element that is not in the tree
    /// yet, the prefix is chosen when the element is inserted. The `xml` prefix needs no
    /// declaration. A character XML cannot carry in `value` is `invalid_payload`.
    pub fn set_attr(
        &mut self,
        el: NodeId,
        ns: &str,
        local: &str,
        value: &str,
    ) -> Result<(), StetError> {
        let n = self.element(el)?;
        if !is_ncname(local) || local == "xmlns" || ns == XMLNS_NS {
            return Err(invalid(
                "namespace declarations are made with bind_namespace",
            ));
        }
        if let Some(i) = self.find_attr(&n, ns, local) {
            let mut list: Vec<Attr> = self.tree.node_attrs(&n).to_vec();
            let Some(a) = list.get(i).copied() else {
                return Err(no_node());
            };
            let full = self.tree.bytes(a.full).to_vec();
            let quote = full.last().copied().unwrap_or(b'"');
            let off = (a.value.start - a.full.start) as usize;
            let escaped = escape_attr(value, quote, self.tree.ascii_only)?;
            let mut text = full.get(..off).unwrap_or(&[]).to_vec();
            text.extend_from_slice(escaped.as_bytes());
            text.push(quote);
            let span = self.tree.add_bytes(&text)?;
            let vs = span.start as usize + off;
            if let Some(slot) = list.get_mut(i) {
                *slot = Attr {
                    full: span,
                    value: Span::new(vs, vs + escaped.len())?,
                    ..a
                };
            }
            self.replace_attrs(el.0, list)?;
            self.flag(el.0, F_TAG_DIRTY);
            return Ok(());
        }
        let escaped = escape_attr(value, b'"', self.tree.ascii_only)?;
        let ns_idx = if ns.is_empty() {
            0
        } else {
            self.tree.intern(ns)?
        };
        // A namespaced attribute needs a prefix, and which one depends on where the element is.
        let (qname, prefix_len): (String, u16) = if ns.is_empty() {
            (local.to_owned(), 0)
        } else if self.tree.in_document(el.0) {
            let p = self.pick_prefix(el.0, ns, false, "ns")?;
            let len = u16::try_from(p.len())
                .ok()
                .filter(|&l| l < Attr::LATE)
                .ok_or_else(|| limit("a prefix is 65535 bytes or more"))?;
            (format!("{p}:{local}"), len)
        } else {
            (local.to_owned(), Attr::LATE)
        };
        let text = format!(" {qname}=\"{escaped}\"");
        let span = self.tree.add_bytes(text.as_bytes())?;
        let name_start = span.start as usize + 1;
        let value_start = name_start + qname.len() + 2;
        let attr = Attr {
            full: span,
            value: Span::new(value_start, value_start + escaped.len())?,
            name: Span::new(name_start, name_start + qname.len())?,
            prefix_len,
            ns: ns_idx,
        };
        // Choosing the prefix may have declared a namespace on this element, so read the list now.
        let n = self.element(el)?;
        let mut list: Vec<Attr> = self.tree.node_attrs(&n).to_vec();
        list.push(attr);
        self.replace_attrs(el.0, list)?;
        self.flag(el.0, F_TAG_DIRTY);
        Ok(())
    }

    /// Removes an attribute. Returns whether it existed. A namespace declaration is not an
    /// attribute to remove: other names in the part may use it.
    pub fn remove_attr(&mut self, el: NodeId, ns: &str, local: &str) -> Result<bool, StetError> {
        let n = self.element(el)?;
        if local == "xmlns" || ns == XMLNS_NS {
            return Err(invalid("a namespace declaration cannot be removed"));
        }
        let Some(i) = self.find_attr(&n, ns, local) else {
            return Ok(false);
        };
        let mut list: Vec<Attr> = self.tree.node_attrs(&n).to_vec();
        list.remove(i);
        self.replace_attrs(el.0, list)?;
        self.flag(el.0, F_TAG_DIRTY);
        Ok(true)
    }

    // ---- text ------------------------------------------------------------------------

    /// Replaces the content of a text node. A character XML cannot carry is `invalid_payload`. A text
    /// node that is a child of the document node, outside the root element, can hold only white
    /// space, which is `invalid_payload` otherwise: any other text there is not a document.
    pub fn set_text(&mut self, node: NodeId, text: &str) -> Result<(), StetError> {
        let n = self.node(node)?;
        if n.kind != NodeKind::Text {
            return Err(invalid("set_text needs a text node"));
        }
        let escaped = if self.at_document_level(n.parent) {
            whitespace_only(text)?.to_owned()
        } else {
            escape_text(text, self.tree.ascii_only)?
        };
        let span = self.tree.add_bytes(escaped.as_bytes())?;
        if let Some(m) = self.tree.node_mut(node.0) {
            m.span = span;
        }
        self.touch(n.parent);
        self.touch(node.0);
        Ok(())
    }

    /// Replaces a range of decoded characters in a text node. Raw bytes outside the range,
    /// including character references such as `&#xA0;`, are kept as written. Positions count
    /// characters as `NodeRef::text` reports them, so a carriage return line feed pair is one. A
    /// character XML cannot carry in `with` is `invalid_payload`, and so is anything but white space
    /// in a text node outside the root element. Where the new text meets what was kept, the bytes
    /// are joined so that they read as the characters they stand for (see `join_bytes`).
    pub fn replace_text_range(
        &mut self,
        node: NodeId,
        start: usize,
        end: usize,
        with: &str,
    ) -> Result<(), StetError> {
        let n = self.node(node)?;
        if n.kind != NodeKind::Text {
            return Err(invalid("replace_text_range needs a text node"));
        }
        let raw = self.tree.str_of(n.span);
        let (a, b) = raw_range_for_chars(raw, start, end)
            .ok_or_else(|| invalid("text range is outside the text node"))?;
        let new = if self.at_document_level(n.parent) {
            whitespace_only(with)?.to_owned()
        } else {
            escape_text(with, self.tree.ascii_only)?
        };
        let mut out: Vec<u8> = Vec::with_capacity(raw.len() + new.len());
        join_bytes(&mut out, raw.get(..a).unwrap_or("").as_bytes());
        join_bytes(&mut out, new.as_bytes());
        join_bytes(&mut out, raw.get(b..).unwrap_or("").as_bytes());
        let span = self.tree.add_bytes(&out)?;
        if let Some(m) = self.tree.node_mut(node.0) {
            m.span = span;
        }
        self.touch(n.parent);
        self.touch(node.0);
        Ok(())
    }

    // ---- creating nodes -----------------------------------------------------------------

    fn push(&mut self, n: Node) -> Result<NodeId, StetError> {
        if self.tree.nodes.len() + self.tree.attrs.len() >= self.tree.max_nodes {
            return Err(limit(format!(
                "the part would have more than {} nodes and attributes",
                self.tree.max_nodes
            )));
        }
        let id = u32::try_from(self.tree.nodes.end())
            .ok()
            .filter(|&i| i != NONE)
            .ok_or_else(|| limit("too many XML nodes"))?;
        self.tree.nodes.push(n);
        self.tree.edits += 1;
        Ok(NodeId(id))
    }

    /// Creates a detached element in a namespace (empty for none). Its prefix is not chosen yet:
    /// it is chosen when the element is inserted, from what is in scope there, so an element can
    /// never take the meaning of a prefix that a closer declaration redefines. Until then
    /// `NodeRef::name` is the local name. An element with no namespace cannot be inserted where a
    /// default namespace is in scope.
    pub fn create_element(&mut self, ns: &str, local: &str) -> Result<NodeId, StetError> {
        if !is_ncname(local) {
            return Err(invalid(format!("{local:?} is not a valid element name")));
        }
        if ns == XMLNS_NS {
            return Err(invalid(
                "elements cannot be in the namespace of declarations",
            ));
        }
        let ns_idx = if ns.is_empty() {
            0
        } else {
            self.tree.intern(ns)?
        };
        let name = self.tree.add_bytes(local.as_bytes())?;
        let mut n = Node::new(NodeKind::Element, Span::default());
        n.name = name;
        n.ns = ns_idx;
        n.flags = F_NEW | F_SUBTREE_DIRTY | F_LATE_NAME;
        n.attr_start =
            u32::try_from(self.tree.attrs.end()).map_err(|_| limit("too many XML attributes"))?;
        self.push(n)
    }

    /// Creates a detached text node. The text is escaped as needed. A character XML cannot carry is
    /// `invalid_payload`.
    pub fn create_text(&mut self, text: &str) -> Result<NodeId, StetError> {
        let escaped = escape_text(text, self.tree.ascii_only)?;
        let span = self.tree.add_bytes(escaped.as_bytes())?;
        self.push(Node::new(NodeKind::Text, span))
    }

    /// Copies a subtree into new, detached nodes. Unedited nodes in the copy are clean, so they
    /// still serialize as their original bytes.
    pub fn clone_subtree(&mut self, id: NodeId) -> Result<NodeId, StetError> {
        let first = self.node(id)?;
        if first.kind == NodeKind::Document {
            return Err(invalid("the document node cannot be cloned"));
        }
        let mut stack: Vec<(u32, u32)> = vec![(id.0, NONE)];
        let mut result = NONE;
        while let Some((old, new_parent)) = stack.pop() {
            let mut n = *self.tree.node(old);
            let attrs: Vec<Attr> = self.tree.node_attrs(&n).to_vec();
            n.attr_start = self
                .tree
                .attrs
                .push_slice(&attrs)
                .and_then(|at| u32::try_from(at).ok())
                .ok_or_else(|| limit("too many XML attributes"))?;
            n.parent = new_parent;
            n.first = NONE;
            n.last = NONE;
            n.next = NONE;
            n.prev = NONE;
            let nid = self.push(n)?.0;
            if new_parent == NONE {
                result = nid;
            } else {
                self.link_last(new_parent, nid);
            }
            let mut kids = Vec::new();
            let mut c = self.tree.node(old).first;
            while c != NONE {
                kids.push(c);
                c = self.tree.node(c).next;
            }
            for c in kids.into_iter().rev() {
                stack.push((c, nid));
            }
        }
        Ok(NodeId(result))
    }

    fn link_last(&mut self, parent: u32, child: u32) {
        let prev = self.tree.node(parent).last;
        if let Some(c) = self.tree.node_mut(child) {
            c.parent = parent;
            c.next = NONE;
            c.prev = prev;
        }
        if let Some(p) = self.tree.node_mut(parent) {
            p.last = child;
            if prev == NONE {
                p.first = child;
            }
        }
        if prev != NONE
            && let Some(q) = self.tree.node_mut(prev)
        {
            q.next = child;
        }
    }

    // ---- structure ------------------------------------------------------------------------

    fn is_ancestor_or_self(&self, anc: u32, mut of: u32) -> bool {
        while of != NONE {
            if of == anc {
                return true;
            }
            of = self.tree.node(of).parent;
        }
        false
    }

    /// Inserts a detached node as a child of `parent`, before `before` or at the end.
    ///
    /// When `parent` is in the document, this is where the prefixes of the new names in the
    /// subtree are chosen, and where its written names are checked against what is in scope. Where a
    /// written prefix would mean something else, the namespace it had is declared on the moved element
    /// (`keep_namespaces`), and only a prefix that cannot be declared is `unsupported`; an element with no namespace
    /// under a default namespace is `invalid_payload`, and a limit that is reached while declaring a
    /// namespace is `limit_exceeded`. So is nesting deeper than the part was parsed with, and so is
    /// putting more namespace declarations in scope than it was parsed with, counting the ones the
    /// subtree brings. Under the document node only comments, processing instructions and white
    /// space can go, and anything else is `invalid_payload`. In each case the subtree stays
    /// detached. A declaration that was already added to the root for an earlier name of the same
    /// subtree stays: it is harmless, and a caller that drops the edit drops the whole tree it
    /// staged.
    pub fn insert_before(
        &mut self,
        parent: NodeId,
        before: Option<NodeId>,
        child: NodeId,
    ) -> Result<(), StetError> {
        let p = self.node(parent)?;
        let c = self.node(child)?;
        if !matches!(p.kind, NodeKind::Element | NodeKind::Document) {
            return Err(invalid("only elements can have children"));
        }
        if c.kind == NodeKind::Document
            || (p.kind == NodeKind::Document && c.kind == NodeKind::Element)
        {
            return Err(invalid("a document has exactly one root element"));
        }
        if c.parent != NONE {
            return Err(invalid("the node is already in the tree: detach it first"));
        }
        if self.is_ancestor_or_self(child.0, parent.0) {
            return Err(invalid("a node cannot become its own descendant"));
        }
        if let Some(b) = before
            && self.node(b)?.parent != parent.0
        {
            return Err(invalid("the reference node is not a child of the parent"));
        }
        self.check_placement(p, before.map(|b| b.0), c)?;
        // Where the parent is not in the document yet, nothing is checked or chosen: that happens
        // when the whole subtree is inserted into the document, because only then is it known what
        // is in scope. Then names that are already written must mean the same thing at the new
        // position, and names the editor has not given a prefix are given one that fits.
        let in_document = self.tree.in_document(parent.0);
        self.check_shape(parent.0, child.0, in_document)?;
        if in_document && self.tree.nested_ns_decls {
            self.keep_namespaces(child.0, parent.0)?;
        }
        let was_empty_self_closing =
            p.first == NONE && p.flags & F_SELF_CLOSING != 0 && p.flags & F_NEW == 0;
        self.link(parent.0, before.map(|b| b.0), child.0);
        if in_document && let Err(e) = self.resolve_late_names(child.0) {
            self.unlink(child.0);
            return Err(e);
        }
        let mut f = F_CHILDREN_DIRTY;
        if was_empty_self_closing {
            f |= F_TAG_DIRTY;
        }
        self.flag(parent.0, f);
        let c = *self.tree.node(child.0);
        self.mark_text_seam(c.prev, child.0);
        self.mark_text_seam(child.0, c.next);
        Ok(())
    }

    /// Marks two text nodes that now meet as changed. The writer joins text that meets
    /// (`join_bytes`), which can write either of the two differently from its bytes, so a node is
    /// dirty exactly when what is written for it can differ from its source.
    fn mark_text_seam(&mut self, left: u32, right: u32) {
        let text = |t: &Self, id: u32| id != NONE && t.tree.node(id).kind == NodeKind::Text;
        if text(self, left) && text(self, right) {
            self.touch(left);
            self.touch(right);
        }
    }

    /// Makes `child` a child of `parent`, before `before` or last. Nothing is marked dirty.
    fn link(&mut self, parent: u32, before: Option<u32>, child: u32) {
        let Some(b) = before else {
            self.link_last(parent, child);
            return;
        };
        let prev = self.tree.node(b).prev;
        if let Some(cn) = self.tree.node_mut(child) {
            cn.parent = parent;
            cn.next = b;
            cn.prev = prev;
        }
        if let Some(bn) = self.tree.node_mut(b) {
            bn.prev = child;
        }
        if prev == NONE {
            if let Some(pn) = self.tree.node_mut(parent) {
                pn.first = child;
            }
        } else if let Some(q) = self.tree.node_mut(prev) {
            q.next = child;
        }
    }

    /// Inserts a detached node as the last child of `parent`.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), StetError> {
        self.insert_before(parent, None, child)
    }

    /// Removes a node from its parent. The node and its subtree stay in the arena and can be
    /// inserted elsewhere. The root element cannot be detached: a document has exactly one, and
    /// without it there is no part to write.
    pub fn detach(&mut self, id: NodeId) -> Result<(), StetError> {
        let n = self.node(id)?;
        if n.parent == NONE {
            return Ok(());
        }
        if id.0 == self.tree.root {
            return Err(invalid(
                "the root element cannot be detached: a document has exactly one",
            ));
        }
        self.unlink(id.0);
        self.flag(n.parent, F_CHILDREN_DIRTY);
        self.mark_text_seam(n.prev, n.next);
        Ok(())
    }

    /// Takes a node out of its parent's list of children. Nothing is marked dirty.
    fn unlink(&mut self, id: u32) {
        let n = *self.tree.node(id);
        if n.parent == NONE {
            return;
        }
        let prev = n.prev;
        if prev == NONE {
            if let Some(p) = self.tree.node_mut(n.parent) {
                p.first = n.next;
            }
        } else if let Some(q) = self.tree.node_mut(prev) {
            q.next = n.next;
        }
        if n.next == NONE {
            if let Some(p) = self.tree.node_mut(n.parent) {
                p.last = prev;
            }
        } else if let Some(q) = self.tree.node_mut(n.next) {
            q.prev = prev;
        }
        if let Some(m) = self.tree.node_mut(id) {
            m.parent = NONE;
            m.next = NONE;
            m.prev = NONE;
        }
    }
}
