//! The reads of one element that `wordml` still does not offer: the kind of a note reference, the note
//! references of a part in order, and what a page number or a legacy date element is.
//!
//! The ids of marks and note references, the revision, control and link of a wrapper, what a drawing is and
//! says, and the text of an equation are `wordml`'s own readers now (`Paragraph::mark_id`, `revision_of`,
//! `control_of`, `hyperlink_of`, `Story::object_info`, `math_text`). What is left reads the element by its
//! name, from a node of a tree that `state` gave, and names no `xml` type.

use crate::projection::model::{AutoKind, NoteKind};
use crate::wordml::{NodeId, SpanTree};

/// Whether a note reference is to a footnote or an endnote: `NoteRef` is both, and the element says which.
pub(super) fn note_kind(tree: &SpanTree, node: NodeId) -> NoteKind {
    if tree.node_ref(node).local_name().starts_with("endnote") {
        NoteKind::Endnote
    } else {
        NoteKind::Footnote
    }
}

/// Every note reference of a tree in document order: the kind, the note's id and the reference's node. References with
/// no usable id are left out, because there is no note they could name.
pub(super) fn note_refs(tree: &SpanTree) -> Vec<(NoteKind, i64, NodeId)> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().children()];
    while let Some(top) = stack.last_mut() {
        let Some(n) = top.next() else {
            stack.pop();
            continue;
        };
        if !n.is_element() {
            continue;
        }
        if matches!(n.local_name(), "footnoteReference" | "endnoteReference") {
            let id = n
                .attrs()
                .find(|a| a.local_name() == "id")
                .and_then(|a| a.value().trim().parse::<i64>().ok());
            if let Some(id) = id {
                out.push((note_kind(tree, n.id()), id, n.id()));
            }
        }
        stack.push(n.children());
    }
    out
}

/// What a page number or a legacy date element is.
pub(super) fn auto_kind(tree: &SpanTree, node: NodeId) -> AutoKind {
    match tree.node_ref(node).local_name() {
        "pgNum" => AutoKind::Page,
        "dayShort" | "monthShort" | "yearShort" | "dayLong" | "monthLong" | "yearLong" => {
            AutoKind::Date
        }
        _ => AutoKind::Value,
    }
}
