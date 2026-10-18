//! W9: what a cell, a row, a table and a note must hold.

use super::{Scan, is_word};
use crate::api::check::FindingCode;
use crate::check::found::Found;
use crate::schema::read_children;
use crate::xml::{NodeId, NodeRef, SpanTree};

/// Elements that mark a place and are not content: a cell that ends with one still ends with what
/// is before it.
const MARKERS: [&str; 19] = [
    "bookmarkStart",
    "bookmarkEnd",
    "commentRangeStart",
    "commentRangeEnd",
    "permStart",
    "permEnd",
    "proofErr",
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
    "customXmlInsRangeStart",
    "customXmlInsRangeEnd",
    "customXmlDelRangeStart",
    "customXmlDelRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveToRangeStart",
    "customXmlMoveToRangeEnd",
];

/// How many block-level content controls and custom XML wrappers a cell's last block is looked
/// for through. Nesting is not limited by the span tree, and the answer for a cell wrapped deeper
/// than this is that it does not end with a paragraph.
const WRAPPERS: usize = 16;

fn is(tree: &SpanTree, node: NodeId, local: &str) -> bool {
    is_word(&tree.node_ref(node), local)
}

fn is_marker(tree: &SpanTree, node: NodeId) -> bool {
    let element = tree.node_ref(node);
    MARKERS.iter().any(|m| is_word(&element, m))
}

/// The last child Word reads that is not a marker, optionally leaving out one more name.
fn last_block(tree: &SpanTree, parent: NodeId, not: &str) -> Option<NodeId> {
    read_children(tree, parent)
        .into_iter()
        .rev()
        .find(|c| !is_marker(tree, *c) && (not.is_empty() || !is(tree, *c, not)))
}

/// Whether the element has a child Word reads with one of these names.
fn holds(tree: &SpanTree, parent: NodeId, names: &[&str]) -> bool {
    read_children(tree, parent)
        .into_iter()
        .any(|c| names.iter().any(|n| is(tree, c, n)))
}

/// Whether the last block of the cell is a paragraph, looking through the wrappers that Word
/// writes around the content of a cell.
fn ends_with_paragraph(tree: &SpanTree, cell: NodeId) -> bool {
    let mut last = last_block(tree, cell, "");
    for _ in 0..WRAPPERS {
        match last {
            Some(n) if is(tree, n, "sdt") => {
                last = read_children(tree, n)
                    .into_iter()
                    .find(|c| is(tree, *c, "sdtContent"))
                    .and_then(|content| last_block(tree, content, ""));
            }
            Some(n) if is(tree, n, "customXml") => last = last_block(tree, n, "customXmlPr"),
            _ => break,
        }
    }
    last.is_some_and(|n| is(tree, n, "p"))
}

/// Whether the element is a note or a comment of its part, and not a reference to one: the child
/// of the root of a notes or comments part.
fn in_notes_part(element: &NodeRef<'_>) -> bool {
    element.parent().is_some_and(|parent| {
        ["footnotes", "endnotes", "comments"]
            .iter()
            .any(|root| is_word(&parent, root))
    })
}

pub(super) fn check(scan: &mut Scan<'_>, element: &NodeRef<'_>, local: &str) {
    let tree = scan.tree;
    let id = element.id();
    let (code, subject) = match local {
        "tc" if !ends_with_paragraph(tree, id) => (FindingCode::CellWithoutFinalParagraph, "tc"),
        "tbl" if !holds(tree, id, &["tr", "sdt", "customXml"]) => {
            (FindingCode::TableWithoutRow, "tbl")
        }
        "tr" if !holds(tree, id, &["tc", "sdt", "customXml"]) => {
            (FindingCode::RowWithoutCell, "tr")
        }
        "footnote" | "endnote" | "comment"
            if in_notes_part(element) && !holds(tree, id, &["p", "tbl", "sdt"]) =>
        {
            (FindingCode::ContainerWithoutBlock, local)
        }
        _ => return,
    };
    scan.found.push(Found::rule(code, subject, Some(id)));
}
