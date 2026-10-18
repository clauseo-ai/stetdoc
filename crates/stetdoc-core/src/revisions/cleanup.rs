//! What a resolution does to the tree besides applying a rule (design `revisions.md`, 3B and 6.2): unwrapping
//! an element, removing one while its range markers stay, renaming deleted text back, pruning the
//! containers that became empty, and finding the block that owns a node.

use super::join::{is_leftover, leftovers};
use super::names::{is_w, is_word_ns};
use super::tree_ops::{child_ids, detach, insert, move_to, next_sibling_of, parent_of, rename};
use crate::api::error::StetError;
use crate::wordml::{NodeId, revision_at};
use crate::xml::SpanTree;

/// True if the node is in the document: its ancestors reach the root element.
pub(super) fn is_attached(tree: &SpanTree, node: NodeId) -> bool {
    let root = tree.root().id();
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if n.id() == root {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// The children of `el` take its place, in order, and `el` goes. Returns the parent.
pub(super) fn unwrap_element(tree: &mut SpanTree, el: NodeId) -> Result<NodeId, StetError> {
    let parent = parent_of(tree, el)
        .ok_or_else(|| super::errors::internal("an unwrapped element has no parent"))?;
    for c in child_ids(tree, el) {
        move_to(tree, c, parent, Some(el))?;
    }
    detach(tree, el)?;
    Ok(parent)
}

/// The ids of the revision elements strictly below a node.
pub(super) fn revision_ids_below(tree: &SpanTree, node: NodeId) -> Vec<i64> {
    let mut out = Vec::new();
    let mut stack = vec![tree.node_ref(node)];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if let Some(r) = revision_at(&c)
                && let Some(id) = r.id
            {
                out.push(id);
            }
            stack.push(c);
        }
    }
    out.sort_unstable();
    out
}

/// The ids of the revision elements at or below a node.
pub(super) fn revision_ids_in(tree: &SpanTree, node: NodeId) -> Vec<i64> {
    let mut out = revision_ids_below(tree, node);
    if let Some(id) = revision_at(&tree.node_ref(node)).and_then(|r| r.id) {
        out.push(id);
    }
    out
}

/// Removes `el` and what it holds, except the range markers and comment references, which move to where
/// `el` was, so that no start loses its end. Returns the parent and the ids of the revisions that went
/// with it.
///
/// A range marker may stand between blocks, but a run may not: when `el` is a block-level element (a
/// row, a cell, a table) and a comment reference run is among what stays, what stays goes to the end of
/// the paragraph before `el` in document order, or the start of the one after it, in order, and that
/// paragraph is what is returned as the parent.
pub(super) fn remove_hoisting(
    tree: &mut SpanTree,
    el: NodeId,
) -> Result<(NodeId, Vec<i64>), StetError> {
    let h = hoist_out(tree, el)?;
    Ok((h.to, h.swallowed))
}

/// What [`hoist_out`] did.
pub(super) struct Hoisted {
    /// Where what stays went: the parent of the removed element, or the paragraph that took it.
    pub to: NodeId,
    /// The ids of the revisions that went with the element.
    pub swallowed: Vec<i64>,
    /// True if `to` is an empty paragraph made to take what stays, because the story had no other.
    pub made: bool,
}

/// [`remove_hoisting`], and whether the paragraph that took what stays was made for it.
pub(super) fn hoist_out(tree: &mut SpanTree, el: NodeId) -> Result<Hoisted, StetError> {
    let parent = parent_of(tree, el)
        .ok_or_else(|| super::errors::internal("a removed element has no parent"))?;
    let mut keep: Vec<NodeId> = Vec::new();
    let mut stack = vec![tree.node_ref(el)];
    // Document order, so the markers keep their order: a depth-first walk with the children reversed.
    while let Some(n) = stack.pop() {
        for c in n.child_elements().collect::<Vec<_>>().into_iter().rev() {
            if is_leftover(&c) {
                keep.push(c.id());
            } else {
                stack.push(c);
            }
        }
    }
    keep.reverse();
    let swallowed: Vec<i64> = revision_ids_below(tree, el)
        .into_iter()
        .filter(|id| {
            !keep
                .iter()
                .any(|k| revision_at(&tree.node_ref(*k)).and_then(|r| r.id) == Some(*id))
        })
        .collect();
    let has_run = keep.iter().any(|k| is_w(&tree.node_ref(*k), "r"));
    let mut to = parent;
    let mut made = false;
    if has_run && !within_paragraph(tree, parent) {
        if let Some(p) = paragraph_beside(tree, el, false) {
            for &k in &keep {
                move_to(tree, k, p, None)?;
            }
            to = p;
        } else if let Some(p) = paragraph_beside(tree, el, true) {
            // After the paragraph's properties, and any white space before them.
            let ppr = tree
                .node_ref(p)
                .child_elements()
                .find(|c| is_w(c, "pPr"))
                .map(|c| c.id());
            let first = match ppr {
                Some(pp) => next_sibling_of(tree, pp),
                None => tree.node_ref(p).first_child().map(|c| c.id()),
            };
            for &k in &keep {
                move_to(tree, k, p, first)?;
            }
            to = p;
        }
        if to == parent {
            // No paragraph is left in the story: one empty paragraph takes them where `el` was, since a
            // story is never left with none (a body that held only this table).
            let uri = super::names::wns(tree);
            let p = super::tree_ops::create_w(tree, &uri, "p")?;
            let story = top_level_of(tree, el);
            let at = story.unwrap_or(el);
            let holder = parent_of(tree, at).unwrap_or(parent);
            insert(tree, holder, Some(at), p)?;
            for &k in &keep {
                move_to(tree, k, p, None)?;
            }
            to = p;
            made = true;
        }
        detach(tree, el)?;
        return Ok(Hoisted {
            to,
            swallowed,
            made,
        });
    }
    for k in keep {
        move_to(tree, k, parent, Some(el))?;
    }
    detach(tree, el)?;
    Ok(Hoisted {
        to: parent,
        swallowed,
        made,
    })
}

/// A story (the body, a cell, a note, a comment, a header, a footer, a text box) that a removal left with no
/// block keeps one empty paragraph, which takes the range markers left between blocks there: a story is never
/// left with none. Returns the paragraph made, if one was.
pub(super) fn keep_a_paragraph(
    tree: &mut SpanTree,
    story: NodeId,
) -> Result<Option<NodeId>, StetError> {
    let (is_story, empty, sect, loose) = {
        let n = tree.node_ref(story);
        let is_story = is_word_ns(n.namespace())
            && matches!(
                n.local_name(),
                "body" | "tc" | "footnote" | "endnote" | "comment" | "hdr" | "ftr" | "txbxContent"
            );
        let empty = !n
            .child_elements()
            .any(|x| is_w(&x, "p") || is_w(&x, "tbl") || is_w(&x, "sdt") || is_w(&x, "customXml"));
        let sect = n
            .child_elements()
            .find(|x| is_w(x, "sectPr"))
            .map(|x| x.id());
        let loose: Vec<NodeId> = n
            .child_elements()
            .filter(|x| is_leftover(x))
            .map(|x| x.id())
            .collect();
        (is_story, empty, sect, loose)
    };
    if !is_story || !empty {
        return Ok(None);
    }
    let uri = super::names::wns(tree);
    let p = super::tree_ops::create_w(tree, &uri, "p")?;
    match sect {
        Some(s) => insert(tree, story, Some(s), p)?,
        None => tree.edit().append_child(story, p)?,
    }
    for l in loose {
        move_to(tree, l, p, None)?;
    }
    Ok(Some(p))
}

/// The outermost block-level ancestor of `el` (or `el`) below its story: where a paragraph can stand in
/// its place.
fn top_level_of(tree: &SpanTree, el: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(el);
    loop {
        let parent = cur.parent()?;
        let story = parent.parent().is_none()
            || (is_word_ns(parent.namespace())
                && matches!(
                    parent.local_name(),
                    "body"
                        | "tc"
                        | "txbxContent"
                        | "footnote"
                        | "endnote"
                        | "comment"
                        | "hdr"
                        | "ftr"
                ));
        if story {
            return Some(cur.id());
        }
        cur = parent;
    }
}

/// True if `node` is a paragraph or lies in one.
fn within_paragraph(tree: &SpanTree, node: NodeId) -> bool {
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if is_w(&n, "p") {
            return true;
        }
        cur = n.parent();
    }
    false
}

/// The paragraph next to `el` in document order within its story, outside it: the one before it, or the
/// one after it if `after`.
fn paragraph_beside(tree: &SpanTree, el: NodeId, after: bool) -> Option<NodeId> {
    // The outermost paragraph of a subtree, first or last in document order.
    fn edge(n: &crate::xml::NodeRef<'_>, after: bool) -> Option<NodeId> {
        let kids: Vec<_> = n.child_elements().collect();
        let ordered: Box<dyn Iterator<Item = _>> = if after {
            Box::new(kids.into_iter())
        } else {
            Box::new(kids.into_iter().rev())
        };
        for c in ordered {
            if is_w(&c, "p") {
                return Some(c.id());
            }
            if let Some(p) = edge(&c, after) {
                return Some(p);
            }
        }
        None
    }
    let mut cur = tree.node_ref(el);
    loop {
        let mut sib = if after {
            cur.next_sibling()
        } else {
            cur.prev_sibling()
        };
        while let Some(s) = sib {
            if s.is_element() {
                if is_w(&s, "p") {
                    return Some(s.id());
                }
                if let Some(p) = edge(&s, after) {
                    return Some(p);
                }
            }
            sib = if after {
                s.next_sibling()
            } else {
                s.prev_sibling()
            };
        }
        let parent = cur.parent()?;
        let story = parent.parent().is_none()
            || (is_word_ns(parent.namespace())
                && matches!(
                    parent.local_name(),
                    "body" | "txbxContent" | "footnote" | "endnote" | "comment" | "hdr" | "ftr"
                ));
        if story || is_w(&parent, "p") {
            return None;
        }
        cur = parent;
    }
}

/// Renames the deleted text below `el` back: `w:delText` to `w:t`, `w:delInstrText` to `w:instrText`.
pub(super) fn rename_text_back(tree: &mut SpanTree, el: NodeId) -> Result<(), StetError> {
    let mut targets: Vec<(NodeId, &'static str)> = Vec::new();
    let mut stack = vec![tree.node_ref(el)];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if is_w(&c, "delText") {
                targets.push((c.id(), "t"));
            } else if is_w(&c, "delInstrText") {
                targets.push((c.id(), "instrText"));
            } else {
                stack.push(c);
            }
        }
    }
    for (id, name) in targets {
        rename(tree, id, name)?;
    }
    Ok(())
}

/// Removes the containers that a removal emptied, from `start` up: an empty `w:rPr`, `w:pPr`, `w:trPr`,
/// `w:tcPr` or `w:numPr`, and an insertion or deletion with nothing in it. The old properties inside a
/// `*PrChange` may be empty, and are left. Returns the node that is left in the tree where the pruning
/// stopped: `start` itself if it stays, and what holds it if it went, which is what a caller that marks
/// `start` as changed marks, because a node that left the tree has no block to touch.
pub(super) fn prune_from(tree: &mut SpanTree, start: NodeId) -> Result<NodeId, StetError> {
    let mut cur = start;
    loop {
        let (empty, prunable, parent) = {
            let n = tree.node_ref(cur);
            let empty = !n.children().any(|c| c.is_element());
            let prunable = is_word_ns(n.namespace())
                && matches!(
                    n.local_name(),
                    "rPr"
                        | "pPr"
                        | "trPr"
                        | "tcPr"
                        | "numPr"
                        | "ins"
                        | "del"
                        | "moveFrom"
                        | "moveTo"
                );
            let parent = n.parent().filter(|p| p.is_element()).map(|p| p.id());
            let in_snapshot = n
                .parent()
                .is_some_and(|p| is_word_ns(p.namespace()) && p.local_name().ends_with("PrChange"));
            (empty && !in_snapshot, prunable, parent)
        };
        if !(empty && prunable) {
            return Ok(cur);
        }
        let Some(parent) = parent else {
            return Ok(cur);
        };
        // A paragraph mark's revision with nothing else: the `w:ins` is the revision itself.
        detach(tree, cur)?;
        cur = parent;
    }
}

/// Which block owns a node, for touching: the paragraph it is in, else the row whose properties or
/// cell properties it is in, else the table whose properties or grid it is in.
pub(super) fn owner_block(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if is_word_ns(n.namespace()) && matches!(n.local_name(), "p" | "tr" | "tbl") {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

/// Moves what a paragraph holds that is not its own content (markers, comment references) into another
/// paragraph, at its start, if the paragraph holds nothing else. Returns true if the paragraph is now
/// empty of content.
pub(super) fn hand_over_leftovers(
    tree: &mut SpanTree,
    from: NodeId,
    to: NodeId,
) -> Result<bool, StetError> {
    let Some(left) = leftovers(tree, from) else {
        return Ok(false);
    };
    let at = tree
        .node_ref(to)
        .child_elements()
        .find(|c| !is_w(c, "pPr"))
        .map(|c| c.id());
    for n in left {
        move_to(tree, n, to, at)?;
    }
    Ok(true)
}

/// Inserts `node` as a child of `parent` before `before` (a thin wrapper kept here for the resolvers).
pub(super) fn put(
    tree: &mut SpanTree,
    parent: NodeId,
    before: Option<NodeId>,
    node: NodeId,
) -> Result<(), StetError> {
    insert(tree, parent, before, node)
}
