//! What an edit may do to the shape of the tree, so that the part it writes can be parsed again
//! under the limits it was parsed with: how deep elements nest, and how many namespace
//! declarations are in scope at once.

use super::edit::Editor;
use super::errors::{invalid, limit};
use super::text::whitespace_only;
use super::tree::{F_NS_DECL, NONE, Node, NodeKind, SpanTree};
use crate::api::error::StetError;

impl SpanTree {
    /// How many elements there are from the top of the tree that holds `id` down to `id`,
    /// counting `id` itself if it is one. The root element is at depth 1.
    pub(super) fn depth_of(&self, mut id: u32) -> usize {
        let mut depth = 0;
        while id != NONE {
            let n = self.node(id);
            depth += usize::from(n.kind == NodeKind::Element);
            id = n.parent;
        }
        depth
    }

    /// The namespace declarations in scope at `id`: those on it and on every ancestor, counting a
    /// declaration that a closer one of the same prefix hides.
    pub(super) fn declarations_in_scope(&self, mut id: u32) -> usize {
        let mut count = 0;
        while id != NONE {
            let n = self.node(id);
            if n.flags & F_NS_DECL != 0 {
                count += self.declarations(n).count();
            }
            id = n.parent;
        }
        count
    }

    /// How deep the subtree at `id` is, and the most declarations in scope at one of its elements
    /// when only the declarations from `id` down count. The depth counts `id` itself if it is an
    /// element, and a text node has depth 0. Goes through the subtree with a stack of its own, so a
    /// subtree nested far beyond the limit cannot overflow the call stack.
    pub(super) fn measure(&self, id: u32) -> (usize, usize) {
        let (mut height, mut declared) = (0, 0);
        let mut stack = vec![(id, 0usize, 0usize)];
        while let Some((id, above, scope)) = stack.pop() {
            let n = self.node(id);
            let here = above + usize::from(n.kind == NodeKind::Element);
            let scope = if n.flags & F_NS_DECL != 0 {
                scope + self.declarations(n).count()
            } else {
                scope
            };
            height = height.max(here);
            declared = declared.max(scope);
            let mut c = n.first;
            while c != NONE {
                stack.push((c, here, scope));
                c = self.node(c).next;
            }
        }
        (height, declared)
    }
}

impl Editor<'_> {
    /// Refuses to put the subtree at `child` under `parent` if it would break a limit the part was
    /// parsed with, before anything is linked:
    /// - its elements would nest deeper than `max_xml_depth`. Where `parent` is not in the document
    ///   yet this is a lower bound, because the subtree can only end up deeper than it is here.
    /// - an element of it would have more namespace declarations in scope than
    ///   `max_xml_namespaces_in_scope`, counting the ones that are in scope at `parent` and the ones
    ///   the subtree brings with it (a copy of an element that declares namespaces, or a node moved
    ///   from somewhere else). This is checked when `parent` is in the document, which is when the
    ///   whole subtree is first measured against what is above it, and it raises the bound that
    ///   `count_declaration` keeps.
    pub(super) fn check_shape(
        &mut self,
        parent: u32,
        child: u32,
        in_document: bool,
    ) -> Result<(), StetError> {
        let (height, declared) = self.tree.measure(child);
        let depth = self.tree.depth_of(parent) + height;
        if depth > self.tree.max_depth {
            return Err(limit(format!(
                "inserting here would nest elements {depth} levels deep, over the limit of {}",
                self.tree.max_depth
            )));
        }
        if in_document && declared > 0 {
            let in_scope = self.tree.declarations_in_scope(parent) + declared;
            if in_scope > self.tree.max_ns_scope {
                return Err(limit(format!(
                    "inserting here would put {in_scope} namespace declarations in scope at once, over the limit of {}",
                    self.tree.max_ns_scope
                )));
            }
            self.tree.ns_peak = self.tree.ns_peak.max(in_scope);
        }
        Ok(())
    }

    /// Counts one more namespace declaration against the limit on declarations in scope, before it
    /// is written. The count is an upper bound: a declaration adds one to the declarations in scope
    /// below it at most, so one more than the most there were is never too few.
    pub(super) fn count_declaration(&mut self) -> Result<(), StetError> {
        if self.tree.ns_peak >= self.tree.max_ns_scope {
            return Err(limit(format!(
                "declaring another namespace would put more than {} declarations in scope at once",
                self.tree.max_ns_scope
            )));
        }
        self.tree.ns_peak += 1;
        Ok(())
    }

    /// True if `parent` is the document node: a text node under it is outside the root element.
    pub(super) fn at_document_level(&self, parent: u32) -> bool {
        parent != NONE && self.tree.node(parent).kind == NodeKind::Document
    }

    /// Whether the node is the XML declaration, a processing instruction whose target is `xml`
    /// (in any case: the name is reserved).
    pub(super) fn is_xml_declaration(&self, n: &Node) -> bool {
        n.kind == NodeKind::ProcessingInstruction
            && self
                .tree
                .str_of(n.span)
                .strip_prefix("<?")
                .and_then(|r| {
                    r.split(|c: char| c.is_ascii_whitespace() || c == '?')
                        .next()
                })
                .is_some_and(|target| target.eq_ignore_ascii_case("xml"))
    }

    /// What a node may be put next to, so that the part is a document:
    /// - the XML declaration cannot be put anywhere (it is only allowed at the very start of the
    ///   part, where it already is or was), and nothing can be put before it.
    /// - under the document node, outside the root element, only comments, processing
    ///   instructions and white space can go. A text node there has to hold white space written as
    ///   itself, because a reference or any other text there is not well-formed, and a CDATA
    ///   section is not allowed there either.
    pub(super) fn check_placement(
        &self,
        parent: Node,
        before: Option<u32>,
        child: Node,
    ) -> Result<(), StetError> {
        if self.is_xml_declaration(&child) {
            return Err(invalid(
                "the XML declaration cannot be inserted: it is only allowed at the start of the part",
            ));
        }
        if parent.kind != NodeKind::Document {
            return Ok(());
        }
        if before.is_some_and(|b| self.is_xml_declaration(self.tree.node(b))) {
            return Err(invalid("nothing can go before the XML declaration"));
        }
        match child.kind {
            NodeKind::Text => whitespace_only(self.tree.str_of(child.span)).map(|_| ()),
            NodeKind::CData => Err(invalid(
                "a CDATA section cannot go outside the root element",
            )),
            _ => Ok(()),
        }
    }
}
