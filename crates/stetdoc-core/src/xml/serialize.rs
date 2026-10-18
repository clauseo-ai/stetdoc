//! The serializer: clean nodes are copied from the source, dirty nodes are re-emitted.
//!
//! See `docs/design/package-and-xml.md` section 2.4. The walk uses an explicit stack, so depth never touches the
//! Rust stack.

use super::encoding::{self, Encoding};
use super::text;
use super::tree::{
    F_CHILDREN_DIRTY, F_NEW, F_SELF_CLOSING, F_SUBTREE_DIRTY, F_TAG_DIRTY, NONE, Node, NodeKind,
    SpanTree,
};

enum Task {
    Node(u32),
    EndTag(u32),
}

impl SpanTree {
    /// The part's bytes. A tree with no edits returns the original bytes exactly. Otherwise
    /// clean nodes are copied from the source and only dirty nodes are re-emitted.
    pub fn serialize(&self) -> Vec<u8> {
        if !self.is_dirty() {
            return match &self.original {
                Some(o) => o.to_vec(),
                None => self.source.to_vec(),
            };
        }
        let out = self.emit(0);
        match self.encoding {
            Encoding::Utf8 => out,
            Encoding::Utf16 { big_endian, bom } => encoding::utf8_to_utf16(&out, big_endian, bom),
        }
    }

    fn emit(&self, from: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.source.len() + self.added.len() + 64);
        if from == 0 {
            out.extend_from_slice(self.source.get(..self.bom_len).unwrap_or(&[]));
        }
        let mut stack = vec![Task::Node(from)];
        while let Some(task) = stack.pop() {
            match task {
                Task::Node(id) => self.enter(id, &mut out, &mut stack),
                Task::EndTag(id) => self.end_tag(id, &mut out),
            }
        }
        out
    }

    fn push_children(&self, n: &Node, stack: &mut Vec<Task>) {
        let mark = stack.len();
        let mut c = n.first;
        while c != NONE {
            stack.push(Task::Node(c));
            c = self.node(c).next;
        }
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }

    fn enter(&self, id: u32, out: &mut Vec<u8>, stack: &mut Vec<Task>) {
        let n = self.node(id);
        match n.kind {
            NodeKind::Document => {
                if n.flags & F_SUBTREE_DIRTY == 0 {
                    out.extend_from_slice(self.bytes(n.span));
                } else {
                    self.push_children(n, stack);
                }
            }
            NodeKind::Element => {
                if n.flags & F_SUBTREE_DIRTY == 0 && n.flags & F_NEW == 0 {
                    out.extend_from_slice(self.bytes(n.span));
                    return;
                }
                let has_children = n.first != NONE;
                self.start_tag(n, has_children, out);
                if has_children {
                    stack.push(Task::EndTag(id));
                    self.push_children(n, stack);
                } else if n.flags & F_NEW == 0 && n.flags & F_SELF_CLOSING == 0 {
                    // A paired element that lost its children keeps its end tag.
                    out.extend_from_slice(
                        self.source
                            .get(n.close_start as usize..n.span.end as usize)
                            .unwrap_or(&[]),
                    );
                }
            }
            // Text goes through `join_bytes`: a text node can follow another one, and bytes that
            // read right apart can read differently when they touch.
            NodeKind::Text => text::join_bytes(out, self.bytes(n.span)),
            _ => out.extend_from_slice(self.bytes(n.span)),
        }
    }

    fn start_tag(&self, n: &Node, has_children: bool, out: &mut Vec<u8>) {
        let is_new = n.flags & F_NEW != 0;
        if !is_new && n.flags & F_TAG_DIRTY == 0 {
            out.extend_from_slice(
                self.source
                    .get(n.span.start as usize..n.open_end as usize)
                    .unwrap_or(&[]),
            );
            return;
        }
        out.push(b'<');
        out.extend_from_slice(self.bytes(n.name));
        for a in self.node_attrs(n) {
            out.extend_from_slice(self.bytes(a.full));
        }
        if is_new {
            out.extend_from_slice(if has_children { b">" } else { b"/>" });
            return;
        }
        let tail = self
            .source
            .get(n.attrs_end as usize..n.open_end as usize)
            .unwrap_or(&[]);
        if n.flags & F_SELF_CLOSING != 0 && has_children {
            // `/>` becomes `>`, keeping any whitespace before it.
            let ws = tail.strip_suffix(b"/>").unwrap_or(tail);
            out.extend_from_slice(ws);
            out.push(b'>');
        } else {
            out.extend_from_slice(tail);
        }
    }

    fn end_tag(&self, id: u32, out: &mut Vec<u8>) {
        let n = self.node(id);
        if n.flags & F_NEW != 0 || n.flags & F_SELF_CLOSING != 0 {
            out.extend_from_slice(b"</");
            out.extend_from_slice(self.bytes(n.name));
            out.push(b'>');
        } else {
            out.extend_from_slice(
                self.source
                    .get(n.close_start as usize..n.span.end as usize)
                    .unwrap_or(&[]),
            );
        }
    }

    /// Marks every element's start tag dirty so that serialization regenerates all of them. The
    /// output must still equal the input, which the tests and the fuzz targets check.
    #[cfg(any(test, feature = "fuzzing"))]
    pub fn mark_all_tags_dirty(&mut self) {
        for n in self.nodes.iter_mut() {
            match n.kind {
                NodeKind::Element => n.flags |= F_TAG_DIRTY | F_CHILDREN_DIRTY | F_SUBTREE_DIRTY,
                NodeKind::Document => n.flags |= F_SUBTREE_DIRTY,
                _ => {}
            }
        }
    }
}
