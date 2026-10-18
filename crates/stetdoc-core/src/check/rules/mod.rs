//! The Word rules that are read inside a unit: what a paragraph, a table or the frame of a part
//! holds. W1, W2, W5, W9 and W10 of `docs/design/check.md` 3.1.
//!
//! A unit is read with `schema::walk_unit`, the walk the validator makes, so the Word rules and
//! the schema see the same elements: the branch Word reads of an `mc:AlternateContent`, no
//! ignorable element, and the paragraphs of a text box. One pass applies every rule, because each
//! is a test on an element and on the elements above it down to its paragraph. [`scan_unit`] reads
//! one unit, and [`scan_part`] a whole part, which is what a document that has never been checked
//! needs. `revisions` runs [`scan_unit`] on the subtrees it created, so the rules have one
//! implementation.

mod revs;
mod tables;
mod text;

use super::found::Found;
use crate::schema::{is_unit_boundary, is_word_namespace, walk_unit};
use crate::xml::{NodeId, NodeRef, SpanTree};

/// What the scan needs to know about an element above the one it is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tag {
    /// A `w:p`: the end of "within one paragraph".
    Paragraph,
    /// A `w:ins`.
    Ins,
    /// A `w:del`.
    Del,
    /// A property container that a revision may be a child of without being a run-level
    /// revision: `w:rPr` (the mark's), `w:trPr`, `w:numPr`.
    Properties,
    /// A `*PrChange`: what is inside it is the old properties, not the document.
    Snapshot,
    /// Anything else, including every element outside the Word namespace.
    Other,
}

/// The tag of a Word element.
pub(super) fn tag_of(local: &str) -> Tag {
    match local {
        "p" => Tag::Paragraph,
        "ins" => Tag::Ins,
        "del" => Tag::Del,
        "rPr" | "trPr" | "numPr" => Tag::Properties,
        "rPrChange" | "pPrChange" | "sectPrChange" | "tblPrChange" | "tblPrExChange"
        | "tblGridChange" | "trPrChange" | "tcPrChange" | "numberingChange" => Tag::Snapshot,
        _ => Tag::Other,
    }
}

/// The tags of the elements above the one being visited, nearest last. The walk reports a depth
/// with each element, and the stack is cut back to it before the element is read.
#[derive(Default)]
pub(super) struct Ancestors(Vec<Tag>);

impl Ancestors {
    /// Forgets everything at `depth` and below, which the walk has left.
    pub(super) fn leave_to(&mut self, depth: usize) {
        self.0.truncate(depth);
    }

    /// The element just read, now an ancestor of what follows.
    pub(super) fn push(&mut self, tag: Tag) {
        self.0.push(tag);
    }

    /// The tag of the parent.
    pub(super) fn parent(&self) -> Option<Tag> {
        self.0.last().copied()
    }

    /// Whether an ancestor has this tag.
    pub(super) fn contains(&self, tag: Tag) -> bool {
        self.0.contains(&tag)
    }

    /// Whether an ancestor between the element and its paragraph has this tag.
    pub(super) fn within_paragraph(&self, tag: Tag) -> bool {
        for t in self.0.iter().rev() {
            if *t == Tag::Paragraph {
                return false;
            }
            if *t == tag {
                return true;
            }
        }
        false
    }
}

/// The scanner: the tree, what is above the element being read, and what it found.
pub(super) struct Scan<'t> {
    pub(super) tree: &'t SpanTree,
    pub(super) up: Ancestors,
    pub(super) found: Vec<Found>,
    /// Whether the walk stops at the paragraphs and tables below the root.
    stop: bool,
}

impl<'t> Scan<'t> {
    fn new(tree: &'t SpanTree, stop: bool) -> Scan<'t> {
        Scan {
            tree,
            up: Ancestors::default(),
            found: Vec::new(),
            stop,
        }
    }

    fn visit(&mut self, node: NodeId, depth: usize) {
        self.up.leave_to(depth);
        // A paragraph or a table below the root is a unit of its own, which the walk visits and
        // does not enter: its rules are its own unit's, and are not read twice.
        if self.stop && depth > 0 && is_unit_boundary(self.tree, node) {
            return;
        }
        let element = self.tree.node_ref(node);
        let tag = if is_word_namespace(element.namespace()) {
            let local = element.local_name();
            text::check(self, &element, local);
            revs::check(self, &element, local);
            tables::check(self, &element, local);
            tag_of(local)
        } else {
            Tag::Other
        };
        self.up.push(tag);
    }
}

/// Whether an element is a Word element with this local name.
pub(super) fn is_word(element: &NodeRef<'_>, local: &str) -> bool {
    element.local_name() == local && is_word_namespace(element.namespace())
}

/// The findings of the Word rules in the unit that starts at `root`: the paragraph, the table or
/// the frame, without the insides of the paragraphs and tables below it.
pub(crate) fn scan_unit(tree: &SpanTree, root: NodeId) -> Vec<Found> {
    scan(tree, root, true)
}

/// The findings of the Word rules in a whole part.
pub(crate) fn scan_part(tree: &SpanTree) -> Vec<Found> {
    scan(tree, tree.root().id(), false)
}

fn scan(tree: &SpanTree, root: NodeId, stop_at_boundaries: bool) -> Vec<Found> {
    let mut scan = Scan::new(tree, stop_at_boundaries);
    walk_unit(tree, root, stop_at_boundaries, &mut |node, depth| {
        scan.visit(node, depth);
    });
    scan.found
}
