//! The span tree: data model and read access.
//!
//! See `docs/design/package-and-xml.md` section 2. Nodes live in one arena and are addressed by `NodeId`. Every
//! node records byte spans into a virtual buffer made of the source bytes followed by an
//! append-only buffer of added bytes, so original and new content are read the same way.

use super::arena::Arena;
use super::bytes::Bytes;
use super::encoding::Encoding;
use super::errors::limit;
use super::nstable::NsTable;
use super::text;
use crate::api::error::StetError;
use std::borrow::Cow;

pub(super) const NONE: u32 = u32::MAX;

pub(super) const F_SELF_CLOSING: u8 = 1;
pub(super) const F_NEW: u8 = 2;
pub(super) const F_TAG_DIRTY: u8 = 4;
pub(super) const F_CHILDREN_DIRTY: u8 = 8;
pub(super) const F_SUBTREE_DIRTY: u8 = 16;
/// The element carries at least one namespace declaration.
pub(super) const F_NS_DECL: u8 = 32;
/// The element was created by the editor and its prefix is chosen when it is inserted.
pub(super) const F_LATE_NAME: u8 = 64;

/// A handle to a node of one tree. It is only meaningful for the tree that issued it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(super) u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// What a node is.
pub enum NodeKind {
    /// The document node, which holds the prolog, the root element and the epilog.
    Document,
    /// An element.
    Element,
    /// Character data between tags.
    Text,
    /// A CDATA section.
    CData,
    /// A comment.
    Comment,
    /// A processing instruction, including the XML declaration.
    ProcessingInstruction,
}

/// A byte range in the virtual buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    /// A span of the virtual buffer. A part, with its edits, cannot be larger than 4 GiB.
    pub(super) fn new(start: usize, end: usize) -> Result<Span, StetError> {
        match (u32::try_from(start), u32::try_from(end)) {
            (Ok(start), Ok(end)) => Ok(Span { start, end }),
            _ => Err(limit(
                "an XML part, with its edits, would be larger than 4 GiB",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Node {
    pub kind: NodeKind,
    pub flags: u8,
    /// Length of the prefix in the qualified name (0 when there is none).
    pub prefix_len: u16,
    pub attr_count: u16,
    /// Index into the namespace table for an element name.
    pub ns: u16,
    pub parent: u32,
    pub first: u32,
    pub last: u32,
    pub next: u32,
    /// The previous sibling, so that taking a node out of a list of children, and inserting before
    /// one, take the same time however long the list is.
    pub prev: u32,
    pub attr_start: u32,
    /// The whole node. For elements, from `<` to the last byte of the end tag.
    pub span: Span,
    /// The qualified name of an element.
    pub name: Span,
    /// Where the start tag ends.
    pub open_end: u32,
    /// Where the attribute area of the start tag ends.
    pub attrs_end: u32,
    /// Where the end tag begins. Equals `span.end` for a self-closing element.
    pub close_start: u32,
}

impl Node {
    pub(super) const EMPTY: Node = Node {
        kind: NodeKind::Text,
        flags: 0,
        prefix_len: 0,
        attr_count: 0,
        ns: 0,
        parent: NONE,
        first: NONE,
        last: NONE,
        next: NONE,
        prev: NONE,
        attr_start: 0,
        span: Span { start: 0, end: 0 },
        name: Span { start: 0, end: 0 },
        open_end: 0,
        attrs_end: 0,
        close_start: 0,
    };

    pub(super) fn new(kind: NodeKind, span: Span) -> Node {
        Node {
            kind,
            span,
            ..Node::EMPTY
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Attr {
    /// From the whitespace before the name to the closing quote.
    pub full: Span,
    /// Inside the quotes.
    pub value: Span,
    pub name: Span,
    /// Length of the prefix in the qualified name, or `Attr::LATE`.
    pub prefix_len: u16,
    pub ns: u16,
}

impl Attr {
    /// `prefix_len` of an attribute the editor made on a node that is not in the tree yet. Its
    /// `name` is the local name only, and its prefix is chosen when the node is inserted.
    pub(super) const LATE: u16 = u16::MAX;

    pub(super) fn is_late(&self) -> bool {
        self.prefix_len == Attr::LATE
    }

    /// The length of the prefix as written. Zero when there is none or it is not chosen yet.
    pub(super) fn written_prefix_len(&self) -> usize {
        if self.is_late() {
            0
        } else {
            usize::from(self.prefix_len)
        }
    }
}

/// A parsed XML part with byte spans. Cloning shares the source bytes.
#[derive(Clone)]
pub struct SpanTree {
    pub(super) source: Bytes,
    /// The original bytes when the part is UTF-16 (the source is then its UTF-8 form).
    pub(super) original: Option<Bytes>,
    pub(super) encoding: Encoding,
    /// Length of the UTF-8 byte-order mark at the start of the source, or 0.
    pub(super) bom_len: usize,
    pub(super) added: Vec<u8>,
    /// The nodes, in chunks that a clone shares until it changes them (`arena.rs`).
    pub(super) nodes: Arena<Node, NODE_SHIFT>,
    /// The attributes of every element, each element's next to each other in one chunk.
    pub(super) attrs: Arena<Attr, ATTR_SHIFT>,
    /// The namespace table. Index 0 is the empty string, meaning no namespace.
    pub(super) ns: NsTable,
    pub(super) root: u32,
    /// Some element below the root declares a namespace.
    pub(super) nested_ns_decls: bool,
    /// The part declares `us-ascii`: new non-ASCII characters are written as references.
    pub(super) ascii_only: bool,
    /// The most attributes one element may have, as when parsing. Edits keep to it, so a part the
    /// editor wrote can be parsed again with the same limits.
    pub(super) max_attrs: usize,
    /// The most nodes plus attributes the arenas may hold, as when parsing.
    pub(super) max_nodes: usize,
    /// The deepest element nesting, as when parsing.
    pub(super) max_depth: usize,
    /// The most namespace declarations in scope at one element, as when parsing.
    pub(super) max_ns_scope: usize,
    /// An upper bound on the declarations in scope at any element: what parsing found, plus one
    /// for each declaration an edit has added since.
    pub(super) ns_peak: usize,
    /// The number of changes the editor has made. See `SpanTree::edit_count`.
    pub(super) edits: u64,
}

/// A chunk of nodes holds 512 of them, 30 KB. An edit copies the chunks that hold the nodes it
/// changes and the elements above them, and a clone shares the rest.
pub(super) const NODE_SHIFT: u32 = 9;
/// A chunk of attributes holds 4,096 of them, 115 KB, and no element has more attributes than that.
pub(super) const ATTR_SHIFT: u32 = 12;

pub(super) const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
pub(super) const XMLNS_NS: &str = "http://www.w3.org/2000/xmlns/";

impl SpanTree {
    pub(super) fn node(&self, id: u32) -> &Node {
        self.nodes.get(id as usize).unwrap_or(&Node::EMPTY)
    }

    pub(super) fn node_mut(&mut self, id: u32) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize)
    }

    /// The bytes of a span in the virtual buffer.
    pub(super) fn bytes(&self, s: Span) -> &[u8] {
        let split = self.source.len();
        let (a, b) = (s.start as usize, s.end as usize);
        let got = if a >= split {
            self.added.get(a - split..b.saturating_sub(split))
        } else {
            self.source.get(a..b)
        };
        got.unwrap_or(&[])
    }

    pub(super) fn str_of(&self, s: Span) -> &str {
        // A source known to be UTF-8 is sliced without a check: a span of a parsed tree starts and
        // ends at a character boundary, and one that does not reads as "" either way.
        if let Some(text) = self.source.as_str()
            && (s.end as usize) <= text.len()
        {
            return text.get(s.start as usize..s.end as usize).unwrap_or("");
        }
        std::str::from_utf8(self.bytes(s)).unwrap_or("")
    }

    pub(super) fn node_attrs(&self, n: &Node) -> &[Attr] {
        self.attrs
            .slice(n.attr_start as usize, usize::from(n.attr_count))
            .unwrap_or(&[])
    }

    pub(super) fn uri(&self, ns: u16) -> &str {
        self.ns.get(ns)
    }

    /// Appends bytes to the added buffer and returns their span in the virtual buffer.
    pub(super) fn add_bytes(&mut self, b: &[u8]) -> Result<Span, StetError> {
        let start = self.source.len() + self.added.len();
        let span = Span::new(start, start + b.len())?;
        self.added.extend_from_slice(b);
        Ok(span)
    }

    pub(super) fn intern(&mut self, uri: &str) -> Result<u16, StetError> {
        self.ns.intern(uri)
    }

    /// The root element.
    pub fn root(&self) -> NodeRef<'_> {
        NodeRef {
            tree: self,
            id: NodeId(self.root),
        }
    }

    /// The document node, whose children are the declaration, comments, whitespace and the
    /// root element.
    pub fn document(&self) -> NodeRef<'_> {
        NodeRef {
            tree: self,
            id: NodeId(0),
        }
    }

    /// Read access to the node with this id.
    pub fn node_ref(&self, id: NodeId) -> NodeRef<'_> {
        NodeRef { tree: self, id }
    }

    /// How many nodes the tree has made, detached ones included. A clone starts at the count of
    /// the tree it was cloned from.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// The nodes made since the tree had `count` nodes ([`SpanTree::node_count`]), detached ones
    /// included: ids only grow. Creating and cloning make nodes; moving and detaching do not.
    pub fn created_since(&self, count: usize) -> impl Iterator<Item = NodeId> + '_ {
        let end = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);
        let start = u32::try_from(count).unwrap_or(u32::MAX);
        (start..end).map(NodeId)
    }

    /// True if any edit has been made.
    pub fn is_dirty(&self) -> bool {
        self.node(0).flags & F_SUBTREE_DIRTY != 0
    }

    /// A number that moves when the editor changes this tree, and only then. It counts marks and not
    /// calls: one call can move it several times (`set_text` marks two nodes, and a `detach` between
    /// two text nodes three), so compare it for equality and never add calls up. A call that changes
    /// nothing (an edit that was refused, a `detach` of a node that is detached, a `bind_namespace`
    /// that finds the binding) leaves it alone, so a read through an `Editor` does not move it.
    /// Creating a node (`create_element`, `create_text`, `clone_subtree`) moves it though no document
    /// changed, which is harmless for a cache. The count is not reset, and a clone starts at the
    /// count of the tree it was cloned from.
    ///
    /// It says that a tree has changed since a count was read, and never that two trees are equal:
    /// two clones of one tree that each make one different edit have the same count. So a cache
    /// records it with the tree's epoch (`state`'s generation of a part is the pair) and compares
    /// both. Only the `Editor` methods promise it: `mark_all_tags_dirty`, which test and fuzz builds
    /// use, changes bytes without moving it.
    pub fn edit_count(&self) -> u64 {
        self.edits
    }
}

/// Read access to one node.
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    pub(super) tree: &'a SpanTree,
    pub(super) id: NodeId,
}

impl<'a> NodeRef<'a> {
    fn n(&self) -> &'a Node {
        self.tree.node(self.id.0)
    }

    fn at(&self, id: u32) -> Option<NodeRef<'a>> {
        (id != NONE).then_some(NodeRef {
            tree: self.tree,
            id: NodeId(id),
        })
    }

    /// The node's handle, for use with the `Editor`.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// What kind of node this is.
    pub fn kind(&self) -> NodeKind {
        self.n().kind
    }

    /// True for an element.
    pub fn is_element(&self) -> bool {
        self.n().kind == NodeKind::Element
    }

    /// The parent node, or `None` for the document node and for detached nodes.
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.at(self.n().parent)
    }

    /// The first child, if any.
    pub fn first_child(&self) -> Option<NodeRef<'a>> {
        self.at(self.n().first)
    }

    /// The last child, if any.
    pub fn last_child(&self) -> Option<NodeRef<'a>> {
        self.at(self.n().last)
    }

    /// The next sibling, if any.
    pub fn next_sibling(&self) -> Option<NodeRef<'a>> {
        self.at(self.n().next)
    }

    /// The previous sibling, if any.
    pub fn prev_sibling(&self) -> Option<NodeRef<'a>> {
        self.at(self.n().prev)
    }

    /// All children, in document order. Whitespace between elements is a text node.
    pub fn children(&self) -> Children<'a> {
        Children {
            next: self.first_child(),
        }
    }

    /// Child elements only.
    pub fn child_elements(&self) -> impl Iterator<Item = NodeRef<'a>> {
        self.children().filter(NodeRef::is_element)
    }

    /// The qualified name as written, such as `w:p`. Empty for non-elements.
    pub fn name(&self) -> &'a str {
        self.tree.str_of(self.n().name)
    }

    /// The prefix as written, or an empty string.
    pub fn prefix(&self) -> &'a str {
        let n = self.name();
        n.get(..usize::from(self.n().prefix_len)).unwrap_or("")
    }

    /// The local name, without the prefix.
    pub fn local_name(&self) -> &'a str {
        let n = self.name();
        let p = usize::from(self.n().prefix_len);
        if p == 0 {
            n
        } else {
            n.get(p + 1..).unwrap_or("")
        }
    }

    /// The namespace URI of the element, or an empty string.
    pub fn namespace(&self) -> &'a str {
        self.tree.uri(self.n().ns)
    }

    /// True if this is an element with the given namespace URI and local name.
    pub fn is(&self, namespace: &str, local: &str) -> bool {
        self.is_element() && self.local_name() == local && self.namespace() == namespace
    }

    /// The attributes in the order they are written, including namespace declarations.
    pub fn attrs(&self) -> impl Iterator<Item = AttrRef<'a>> {
        let tree = self.tree;
        tree.node_attrs(self.n())
            .iter()
            .map(move |a| AttrRef { tree, a })
    }

    /// The decoded value of an attribute by namespace URI (empty for none) and local name.
    pub fn attr(&self, namespace: &str, local: &str) -> Option<Cow<'a, str>> {
        self.attrs()
            .find(|a| a.local_name() == local && a.namespace() == namespace)
            .map(|a| a.value())
    }

    /// The decoded text of a text, CDATA or comment node, as an XML parser reports it: character
    /// references are decoded in text, and a carriage return line feed pair or a lone carriage
    /// return is a line feed. A character written as a reference is kept as it is. Empty for other
    /// kinds.
    pub fn text(&self) -> Cow<'a, str> {
        let s = self.tree.str_of(self.n().span);
        match self.n().kind {
            NodeKind::Text => text::unescape_text(s),
            NodeKind::CData => text::normalize_line_endings(
                s.strip_prefix("<![CDATA[")
                    .and_then(|x| x.strip_suffix("]]>"))
                    .unwrap_or(""),
            ),
            NodeKind::Comment => text::normalize_line_endings(
                s.strip_prefix("<!--")
                    .and_then(|x| x.strip_suffix("-->"))
                    .unwrap_or(""),
            ),
            _ => Cow::Borrowed(""),
        }
    }

    /// All text and CDATA below this node, in document order.
    pub fn text_content(&self) -> String {
        let mut out = String::new();
        let mut stack = vec![*self];
        while let Some(n) = stack.pop() {
            match n.kind() {
                NodeKind::Text | NodeKind::CData => out.push_str(&n.text()),
                NodeKind::Element | NodeKind::Document => {
                    let mark = stack.len();
                    stack.extend(n.children());
                    if let Some(tail) = stack.get_mut(mark..) {
                        tail.reverse();
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// The node's original bytes, from the source. For a node an edit created, the bytes
    /// are empty. For a node that was edited, these are the bytes before the edit.
    pub fn source_bytes(&self) -> &'a [u8] {
        let n = self.n();
        if n.flags & F_NEW != 0 || (n.span.start as usize) >= self.tree.source.len() {
            return &[];
        }
        self.tree.bytes(n.span)
    }

    /// The byte offsets of this node in the tree's source, or `None` for a node an edit
    /// created or replaced. For a UTF-16 part the offsets are in the UTF-8 form.
    pub fn source_span(&self) -> Option<(usize, usize)> {
        let n = self.n();
        if n.flags & F_NEW != 0 || (n.span.start as usize) >= self.tree.source.len() {
            return None;
        }
        Some((n.span.start as usize, n.span.end as usize))
    }

    /// True if this node or anything below it was changed, or if an edit made this text node meet
    /// another one, which the writer joins and can write differently. A node that is not dirty is
    /// written exactly as it was read.
    pub fn is_dirty(&self) -> bool {
        self.n().flags & F_SUBTREE_DIRTY != 0
    }

    /// True if a direct text or CDATA child holds something other than whitespace.
    pub fn has_text(&self) -> bool {
        self.children().any(|c| {
            matches!(c.kind(), NodeKind::Text | NodeKind::CData)
                && !c.text().chars().all(char::is_whitespace)
        })
    }

    /// The namespace URI that `prefix` means at this node, found by walking up through the
    /// declarations in scope. An empty prefix asks for the default namespace, and an empty result
    /// means it was undeclared. `None` if the prefix is not bound.
    pub fn namespace_of_prefix(&self, prefix: &str) -> Option<String> {
        self.tree
            .prefix_uri_at(self.id.0, prefix)
            .map(Cow::into_owned)
    }
}

/// Iterator over the children of a node.
pub struct Children<'a> {
    next: Option<NodeRef<'a>>,
}

impl<'a> Iterator for Children<'a> {
    type Item = NodeRef<'a>;

    fn next(&mut self) -> Option<NodeRef<'a>> {
        let cur = self.next?;
        self.next = cur.next_sibling();
        Some(cur)
    }
}

/// Read access to one attribute.
#[derive(Clone, Copy)]
pub struct AttrRef<'a> {
    tree: &'a SpanTree,
    a: &'a Attr,
}

impl std::fmt::Debug for NodeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NodeRef({:?} {:?} #{})",
            self.kind(),
            self.name(),
            self.id.0
        )
    }
}

impl PartialEq for NodeRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.tree, other.tree) && self.id == other.id
    }
}

impl Eq for NodeRef<'_> {}

impl<'a> AttrRef<'a> {
    /// True for `xmlns` and `xmlns:prefix` attributes, which declare namespaces and are not data.
    pub fn is_namespace_declaration(&self) -> bool {
        self.a.ns != 0 && self.namespace() == XMLNS_NS
    }

    /// The attribute name as written, such as `w:val`.
    pub fn name(&self) -> &'a str {
        self.tree.str_of(self.a.name)
    }

    /// The local name, without the prefix.
    pub fn local_name(&self) -> &'a str {
        let n = self.name();
        match self.a.written_prefix_len() {
            0 => n,
            p => n.get(p + 1..).unwrap_or(""),
        }
    }

    /// The namespace URI, or an empty string for none.
    pub fn namespace(&self) -> &'a str {
        self.tree.uri(self.a.ns)
    }

    /// The decoded value, as an XML parser reports it: references are decoded, and a tab, line
    /// feed or carriage return written literally is a space (a carriage return line feed pair is
    /// one space). A character written as a reference is kept as it is.
    pub fn value(&self) -> Cow<'a, str> {
        text::unescape_attr(self.tree.str_of(self.a.value))
    }

    /// The value as written, with references undecoded.
    pub fn raw_value(&self) -> &'a [u8] {
        self.tree.bytes(self.a.value)
    }
}
