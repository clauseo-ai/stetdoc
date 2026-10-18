//! The scanner that builds a span tree from the bytes of one XML part.
//!
//! One pass, no recursion. Strict about structure (tags, quotes, names, attributes,
//! references, namespaces, depth) and lenient about content rules that do not affect the
//! structure. A DOCTYPE is rejected without being read.

use super::arena::Builder;
use super::bytes::Bytes;
use super::chars;
use super::encoding::{self, Encoding};
use super::errors::{limit, malformed, with_part};
use super::nstable::NsTable;
use super::scan_names::Recent;
use super::scope::ScopeStack;
use super::text::{check_references, find_cdata_end};
use super::tree::{
    ATTR_SHIFT, Attr, F_NS_DECL, F_SELF_CLOSING, NODE_SHIFT, NONE, Node, NodeKind, Span, SpanTree,
};
use crate::api::error::StetError;
use crate::api::limits::Limits;

/// What follows a byte offset in a message when the part was UTF-16: the parser works on the
/// UTF-8 form of the part, so its offsets do not match the bytes of the file.
const UTF16_NOTE: &str = " of the UTF-8 form of this UTF-16 part";

/// An element holds at most 4,096 attributes, whatever the limit says: the attributes of an element
/// are stored next to each other in one chunk of the arena, and the tree counts them in 16 bits with
/// the all-ones value reserved, which is a larger number than a chunk holds.
pub(super) const MOST_ATTRIBUTES: usize = {
    let chunk = 1usize << ATTR_SHIFT;
    let counted = (Attr::LATE - 1) as usize;
    if chunk < counted { chunk } else { counted }
};

/// A byte offset in the part's source, as an offset can be read in a message.
fn offset(src_off: usize) -> Result<u32, StetError> {
    u32::try_from(src_off).map_err(|_| limit("an XML part is larger than 4 GiB"))
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':' || b >= 0x80
}

fn is_name_char(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit() || b == b'-' || b == b'.'
}

pub(super) struct Open {
    id: u32,
    /// The scope marker from before this element's own declarations.
    mark: usize,
    /// The qualified name, which the end tag has to match.
    name: Span,
    /// The last child so far. A node is linked to the one before it through this, and the element
    /// is told which child came last when it closes, so that neither takes a look at the element.
    last: u32,
}

pub(super) struct Parser<'a> {
    pub(super) src: &'a [u8],
    pub(super) pos: usize,
    pub(super) limits: &'a Limits,
    pub(super) nodes: Builder<Node, NODE_SHIFT>,
    pub(super) attrs: Builder<Attr, ATTR_SHIFT>,
    pub(super) ns: NsTable,
    /// What each prefix means at the current position.
    pub(super) scope: ScopeStack<&'a [u8], u16>,
    /// The last few answers from `scope`, for the common case where a part declares its
    /// namespaces once and every name after that uses one of a handful of prefixes.
    pub(super) recent: Recent<'a>,
    pub(super) xmlns_idx: u16,
    pub(super) xml_idx: u16,
    pub(super) stack: Vec<Open>,
    /// The last child of the document node so far, as `Open::last` is for an element.
    pub(super) doc_last: u32,
    pub(super) nested_ns_decls: bool,
    /// The most declarations that were in scope at once so far.
    pub(super) ns_peak: usize,
    pub(super) ascii_only: bool,
    pub(super) root: u32,
    /// Appended to a byte offset in a message: empty, or a note that the part was UTF-16.
    pub(super) note: &'static str,
}

/// Parses a part. `bytes` is the part as stored (any encoding the rules allow). `part` is its name,
/// which every error message starts with.
pub(super) fn parse(part: &str, bytes: Bytes, limits: &Limits) -> Result<SpanTree, StetError> {
    parse_part(bytes, limits).map_err(|e| with_part(e, part))
}

fn parse_part(bytes: Bytes, limits: &Limits) -> Result<SpanTree, StetError> {
    if bytes.len() > (u32::MAX / 2) as usize {
        return Err(limit("XML part is larger than 2 GiB"));
    }
    let (enc, bom_len) = encoding::detect(&bytes)?;
    let utf16 = matches!(enc, Encoding::Utf16 { .. });
    let note = if utf16 { UTF16_NOTE } else { "" };
    let (source, original, bom_len): (Bytes, Option<Bytes>, usize) = match enc {
        Encoding::Utf8 => (bytes, None, bom_len),
        Encoding::Utf16 { big_endian, .. } => {
            let body = bytes.get(bom_len..).unwrap_or(&[]);
            let utf8 = encoding::utf16_to_utf8(body, big_endian)?;
            (Bytes::from(utf8), Some(bytes), 0)
        }
    };
    if source.len() > u32::MAX as usize {
        return Err(limit("XML part is larger than 4 GiB once decoded"));
    }
    let body = source.get(bom_len..).unwrap_or(&[]);
    if let Err(e) = std::str::from_utf8(body) {
        return Err(malformed(format!(
            "malformed XML at byte {}{note}: invalid UTF-8",
            bom_len + e.valid_up_to()
        )));
    }
    if let Some((at, c)) = chars::first_forbidden(body) {
        return Err(malformed(format!(
            "malformed XML at byte {}{note}: U+{c:04X} is not a character XML 1.0 allows",
            bom_len + at
        )));
    }
    // The arenas grow a chunk at a time, so nothing is guessed and nothing is copied to make room.
    let mut p = Parser {
        src: &source,
        pos: bom_len,
        limits,
        nodes: Builder::new(),
        attrs: Builder::new(),
        ns: NsTable::new(),
        scope: ScopeStack::new(),
        recent: Recent::default(),
        xmlns_idx: 0,
        xml_idx: 0,
        stack: Vec::new(),
        doc_last: NONE,
        nested_ns_decls: false,
        ns_peak: 0,
        ascii_only: false,
        root: NONE,
        note,
    };
    p.nodes.push(Node::new(
        NodeKind::Document,
        Span::new(bom_len, source.len())?,
    ));
    p.run(enc)?;
    if let Some(document) = p.nodes.get_mut(0) {
        document.last = p.doc_last;
    }
    let (nodes, attrs, ns, nested, root, ascii_only, ns_peak) = (
        p.nodes.freeze(),
        p.attrs.freeze(),
        p.ns,
        p.nested_ns_decls,
        p.root,
        p.ascii_only,
        p.ns_peak,
    );
    Ok(SpanTree {
        source,
        original,
        encoding: enc,
        bom_len,
        added: Vec::new(),
        nodes,
        attrs,
        ns,
        root,
        nested_ns_decls: nested,
        ascii_only,
        max_attrs: (limits.max_xml_attributes as usize).min(MOST_ATTRIBUTES),
        max_nodes: limits.max_xml_nodes as usize,
        max_depth: limits.max_xml_depth as usize,
        max_ns_scope: limits.max_xml_namespaces_in_scope as usize,
        ns_peak,
        edits: 0,
    })
}

impl Parser<'_> {
    /// A `malformed_input` error at a byte offset of the source.
    pub(super) fn bad(&self, src_off: usize, what: &str) -> StetError {
        malformed(format!(
            "malformed XML at byte {src_off}{}: {what}",
            self.note
        ))
    }

    fn push_node(&mut self, n: Node) -> Result<u32, StetError> {
        if self.nodes.len().saturating_add(self.attrs.len()) >= self.limits.max_xml_nodes as usize {
            return Err(limit(format!(
                "XML part has more than {} nodes and attributes",
                self.limits.max_xml_nodes
            )));
        }
        let id = u32::try_from(self.nodes.end()).map_err(|_| limit("too many XML nodes"))?;
        // The parent is the open element, or the document node. It keeps the last child it has,
        // so the node is linked to the one before it, and the parent is read only to be told of its
        // first child. Its last child is written when it closes.
        let (parent, prev) = match self.stack.last_mut() {
            Some(open) => (open.id, std::mem::replace(&mut open.last, id)),
            None => (0, std::mem::replace(&mut self.doc_last, id)),
        };
        let mut n = n;
        n.parent = parent;
        n.prev = prev;
        self.nodes.push(n);
        if let Some(m) = self
            .nodes
            .get_mut(if prev == NONE { parent } else { prev } as usize)
        {
            if prev == NONE {
                m.first = id;
            } else {
                m.next = id;
            }
        }
        Ok(id)
    }

    fn find(&self, from: usize, needle: &[u8]) -> Option<usize> {
        memchr::memmem::find(self.src.get(from..)?, needle).map(|i| from + i)
    }

    fn run(&mut self, enc: Encoding) -> Result<(), StetError> {
        let len = self.src.len();
        let first = self.pos;
        while self.pos < len {
            let at = self.pos;
            if self.src.get(at) != Some(&b'<') {
                self.text(at)?;
                continue;
            }
            match self.src.get(at + 1).copied() {
                Some(b'/') => self.end_tag(at)?,
                Some(b'?') => self.pi(at, first, enc)?,
                Some(b'!') => self.bang(at)?,
                Some(_) => self.start_tag(at)?,
                None => return Err(self.bad(at, "'<' at the end of the part")),
            }
        }
        if let Some(open) = self.stack.last() {
            let name = self.name_of(open.name);
            return Err(self.bad(len, &format!("element <{name}> is never closed")));
        }
        if self.root == NONE {
            return Err(self.bad(len, "no root element"));
        }
        Ok(())
    }

    fn name_of(&self, s: Span) -> String {
        self.src
            .get(s.start as usize..s.end as usize)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .unwrap_or_default()
    }

    fn text(&mut self, at: usize) -> Result<(), StetError> {
        let rest = self.src.get(at..).unwrap_or(&[]);
        let end = at + memchr::memchr(b'<', rest).unwrap_or(rest.len());
        let raw = self.src.get(at..end).unwrap_or(&[]);
        if self.stack.is_empty() {
            if !raw.iter().all(|&b| is_ws(b)) {
                return Err(self.bad(at, "text outside the root element"));
            }
        } else if let Err(off) = check_references(raw) {
            return Err(self.bad(at + off, "undefined or malformed entity reference"));
        } else if let Some(off) = find_cdata_end(raw) {
            return Err(self.bad(
                at + off,
                "the text contains ]]>, which only ends a CDATA section: write the > as &gt;",
            ));
        }
        self.push_node(Node::new(NodeKind::Text, Span::new(at, end)?))?;
        self.pos = end;
        Ok(())
    }

    fn bang(&mut self, at: usize) -> Result<(), StetError> {
        let rest = self.src.get(at..).unwrap_or(&[]);
        if rest.starts_with(b"<!--") {
            let end = self
                .find(at + 4, b"-->")
                .ok_or_else(|| self.bad(at, "unterminated comment"))?
                + 3;
            self.push_node(Node::new(NodeKind::Comment, Span::new(at, end)?))?;
            self.pos = end;
            Ok(())
        } else if rest.starts_with(b"<![CDATA[") {
            if self.stack.is_empty() {
                return Err(self.bad(at, "CDATA outside the root element"));
            }
            let end = self
                .find(at + 9, b"]]>")
                .ok_or_else(|| self.bad(at, "unterminated CDATA section"))?
                + 3;
            self.push_node(Node::new(NodeKind::CData, Span::new(at, end)?))?;
            self.pos = end;
            Ok(())
        } else {
            Err(self.bad(at, "DOCTYPE and entity declarations are not allowed"))
        }
    }

    fn pi(&mut self, at: usize, first: usize, enc: Encoding) -> Result<(), StetError> {
        let end = self
            .find(at + 2, b"?>")
            .ok_or_else(|| self.bad(at, "unterminated processing instruction"))?
            + 2;
        let body = self.src.get(at + 2..end - 2).unwrap_or(&[]);
        let target_len = body.iter().position(|&b| is_ws(b)).unwrap_or(body.len());
        let target = body.get(..target_len).unwrap_or(&[]);
        if target.is_empty() || !target.first().is_some_and(|&b| is_name_start(b)) {
            return Err(self.bad(at, "processing instruction without a target"));
        }
        if target.eq_ignore_ascii_case(b"xml") {
            if at != first || target != b"xml" {
                return Err(self.bad(
                    at,
                    "the XML declaration is only allowed at the start of the part",
                ));
            }
            let text = std::str::from_utf8(body).unwrap_or("");
            // OPC parts are XML 1.0. XML 1.1 allows control characters that 1.0 does not, and
            // Word refuses it.
            match pseudo_attr(text, "version") {
                Some("1.0") => {}
                Some(other) => {
                    return Err(self.bad(
                        at,
                        &format!("XML version {other:?} is not allowed: a part is XML 1.0"),
                    ));
                }
                None => return Err(self.bad(at, "the XML declaration has no version")),
            }
            let declared = encoding::check_declared(pseudo_attr(text, "encoding"), enc)?;
            if declared == encoding::Declared::AsciiOnly {
                if let Some(off) = self.src.iter().position(|b| !b.is_ascii()) {
                    return Err(self.bad(
                        off,
                        "the XML declaration says us-ascii but this byte is not ASCII",
                    ));
                }
                self.ascii_only = true;
            }
        }
        self.push_node(Node::new(
            NodeKind::ProcessingInstruction,
            Span::new(at, end)?,
        ))?;
        self.pos = end;
        Ok(())
    }

    fn end_tag(&mut self, at: usize) -> Result<(), StetError> {
        let rest = self.src.get(at..).unwrap_or(&[]);
        let gt = memchr::memchr(b'>', rest).ok_or_else(|| self.bad(at, "unterminated end tag"))?;
        let inner = rest.get(2..gt).unwrap_or(&[]);
        let name_len = inner.iter().position(|&b| is_ws(b)).unwrap_or(inner.len());
        if !inner
            .get(name_len..)
            .unwrap_or(&[])
            .iter()
            .all(|&b| is_ws(b))
        {
            return Err(self.bad(at, "unexpected characters in an end tag"));
        }
        let open = self
            .stack
            .pop()
            .ok_or_else(|| self.bad(at, "end tag with no open element"))?;
        let want = self
            .src
            .get(open.name.start as usize..open.name.end as usize)
            .unwrap_or(&[]);
        if want != inner.get(..name_len).unwrap_or(&[]) {
            return Err(self.bad(
                at,
                &format!(
                    "end tag </{}> does not match <{}>",
                    String::from_utf8_lossy(inner.get(..name_len).unwrap_or(&[])),
                    self.name_of(open.name)
                ),
            ));
        }
        let end = at + gt + 1;
        let (close_start, end32) = (offset(at)?, offset(end)?);
        let Some(n) = self.nodes.get_mut(open.id as usize) else {
            return Err(self.bad(at, "internal: lost open element"));
        };
        n.close_start = close_start;
        n.span.end = end32;
        n.last = open.last;
        self.scope.close(open.mark);
        self.pos = end;
        Ok(())
    }

    fn start_tag(&mut self, at: usize) -> Result<(), StetError> {
        let src = self.src;
        if self.stack.is_empty() && self.root != NONE {
            return Err(self.bad(at, "more than one root element"));
        }
        if self.stack.len() >= self.limits.max_xml_depth as usize {
            return Err(limit(format!(
                "XML nesting is deeper than {} levels (byte {at}{})",
                self.limits.max_xml_depth, self.note
            )));
        }
        let mut p = at + 1;
        let name_start = p;
        if !src.get(p).is_some_and(|&b| is_name_start(b)) {
            return Err(self.bad(at, "'<' is not followed by an element name"));
        }
        while src.get(p).is_some_and(|&b| is_name_char(b)) {
            p += 1;
        }
        let name = Span::new(name_start, p)?;
        // The attributes are stored as they are read, one list in one chunk: `attr_first` is where
        // the list starts, and moves if it has to go to the next chunk.
        let mut attr_first = self.attrs.end();
        let mut attr_count = 0usize;
        let (self_closing, open_end, attrs_end);
        loop {
            let ws_start = p;
            while src.get(p).is_some_and(|&b| is_ws(b)) {
                p += 1;
            }
            match src.get(p).copied() {
                None => return Err(self.bad(at, "unterminated start tag")),
                Some(b'>') => {
                    self_closing = false;
                    attrs_end = ws_start;
                    open_end = p + 1;
                    break;
                }
                Some(b'/') => {
                    if src.get(p + 1) != Some(&b'>') {
                        return Err(self.bad(p, "'/' in a start tag must be followed by '>'"));
                    }
                    self_closing = true;
                    attrs_end = ws_start;
                    open_end = p + 2;
                    break;
                }
                Some(b) if is_name_start(b) => {}
                Some(_) => return Err(self.bad(p, "unexpected character in a start tag")),
            }
            if p == ws_start {
                return Err(self.bad(p, "attributes must be separated by whitespace"));
            }
            let most = (self.limits.max_xml_attributes as usize).min(MOST_ATTRIBUTES);
            if attr_count >= most {
                return Err(limit(format!(
                    "an element has more than {most} attributes (byte {at}{})",
                    self.note
                )));
            }
            let a_name_start = p;
            while src.get(p).is_some_and(|&b| is_name_char(b)) {
                p += 1;
            }
            let a_name = Span::new(a_name_start, p)?;
            while src.get(p).is_some_and(|&b| is_ws(b)) {
                p += 1;
            }
            if src.get(p) != Some(&b'=') {
                return Err(self.bad(p, "attribute has no '='"));
            }
            p += 1;
            while src.get(p).is_some_and(|&b| is_ws(b)) {
                p += 1;
            }
            let q = match src.get(p).copied() {
                Some(q @ (b'"' | b'\'')) => q,
                _ => return Err(self.bad(p, "attribute value is not quoted")),
            };
            p += 1;
            let v_start = p;
            let rest = src.get(p..).unwrap_or(&[]);
            let close = memchr::memchr(q, rest)
                .ok_or_else(|| self.bad(v_start, "unterminated attribute value"))?;
            let value = rest.get(..close).unwrap_or(&[]);
            if memchr::memchr(b'<', value).is_some() {
                return Err(self.bad(v_start, "'<' in an attribute value"));
            }
            if let Err(off) = check_references(value) {
                return Err(self.bad(v_start + off, "undefined or malformed entity reference"));
            }
            p = v_start + close + 1;
            let attr = Attr {
                full: Span::new(ws_start, p)?,
                value: Span::new(v_start, v_start + close)?,
                name: a_name,
                prefix_len: 0,
                ns: 0,
            };
            if !self.attrs.push_listed(&mut attr_first, attr) {
                return Err(limit("an element has too many attributes"));
            }
            attr_count += 1;
        }
        self.check_duplicate_attrs(attr_first, attr_count, at)?;

        // Namespace declarations, then name resolution.
        let mark = self.scope.mark();
        let declared = self.declare_namespaces(attr_first, attr_count, at)?;
        if declared > 0 && !self.stack.is_empty() {
            self.nested_ns_decls = true;
        }
        let (prefix_len, ns) = self.resolve_element(name, at)?;
        for i in attr_first..attr_first + attr_count {
            let Some(a) = self.attrs.get(i).copied() else {
                continue;
            };
            let (pl, ns) = self.resolve_attr(a.name, at)?;
            if let Some(slot) = self.attrs.get_mut(i) {
                slot.prefix_len = pl;
                slot.ns = ns;
            }
        }
        self.check_duplicate_expanded(attr_first, attr_count, at)?;

        let mut node = Node::new(NodeKind::Element, Span::new(at, open_end)?);
        node.name = name;
        node.prefix_len = prefix_len;
        node.ns = ns;
        node.attr_start = offset(attr_first)?;
        node.attr_count =
            u16::try_from(attr_count).map_err(|_| limit("an element has too many attributes"))?;
        node.open_end = offset(open_end)?;
        node.attrs_end = offset(attrs_end)?;
        node.close_start = node.open_end;
        if self_closing {
            node.flags |= F_SELF_CLOSING;
        }
        if declared > 0 {
            node.flags |= F_NS_DECL;
        }
        let was_empty = self.stack.is_empty();
        let id = self.push_node(node)?;
        if was_empty {
            self.root = id;
        }
        if self_closing {
            self.scope.close(mark);
        } else {
            self.stack.push(Open {
                id,
                mark,
                name,
                last: NONE,
            });
        }
        self.pos = open_end;
        Ok(())
    }
}

/// Reads a pseudo-attribute (`encoding`, `version`) from the body of an XML declaration.
fn pseudo_attr<'a>(decl: &'a str, name: &str) -> Option<&'a str> {
    let i = decl.find(name)?;
    let rest = decl.get(i + name.len()..)?.trim_start();
    let rest = rest.strip_prefix('=')?.trim_start();
    let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = rest.get(1..)?;
    body.get(..body.find(q)?)
}
