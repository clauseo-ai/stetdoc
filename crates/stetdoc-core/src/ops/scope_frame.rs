//! What changed in the frame of a story part, for the second pass of the scope guard (design `document-and-ops.md` 3.3).
//!
//! The frame is what no block holds: the root, the body, the body's last section properties, and what lies between blocks
//! (block-level content controls, markers). `check` reports the frame of a story part as changed when any of it is not the
//! base's, which a paragraph inserted into the body does too, so this module says what differs in it and which block, if any,
//! each difference belongs to. A difference between blocks belongs to the first block at or after it, as `owner_entry`
//! gives a revision there. The root's and the body's own attributes and the body's last section properties belong to no block.

use crate::wordml::{NodeId, SpanTree, Story};
use std::collections::HashSet;

/// One difference in the frame, and the block it belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrameChange {
    /// It lies before this block, or inside an element that holds it, and the story still has the block.
    Before(NodeId),
    /// It belongs to no block: the root's or the body's own attributes, the body's last section properties, or something after
    /// the last block.
    Unowned,
}

/// True for an element that is a block of its own, which the frame does not hold.
fn is_block(tree: &SpanTree, id: NodeId) -> bool {
    let n = tree.node_ref(id);
    n.is_element() && matches!(n.local_name(), "p" | "tbl")
}

/// True for whitespace between elements, which a writer may add or drop.
fn is_space(tree: &SpanTree, id: NodeId) -> bool {
    let n = tree.node_ref(id);
    !n.is_element() && n.text().chars().all(char::is_whitespace)
}

/// The attributes of an element as a sorted list, so that the order they were written in does not matter.
fn attributes(tree: &SpanTree, id: NodeId) -> Vec<(&str, &str, &[u8])> {
    let mut list: Vec<_> = tree
        .node_ref(id)
        .attrs()
        .map(|x| (x.namespace(), x.local_name(), x.raw_value()))
        .collect();
    list.sort_unstable();
    list
}

/// True when the node is the body's section properties or inside them. A revision element there, and the change that owns
/// it, is left out of what a scope's `accept` and `reject` can take, because no block holds it.
pub(super) fn in_body_section(tree: &SpanTree, id: NodeId) -> bool {
    let mut at = Some(tree.node_ref(id));
    while let Some(x) = at {
        if x.is_element()
            && x.local_name() == "sectPr"
            && x.parent().is_some_and(|p| p.local_name() == "body")
        {
            return true;
        }
        at = x.parent();
    }
    false
}

/// The first block of the story at or after the node, in document order of the tree the node is in.
fn owner(story: &Story, tree: &SpanTree, id: NodeId) -> FrameChange {
    if in_body_section(tree, id) {
        return FrameChange::Unowned;
    }
    let first_in = |start: NodeId| -> Option<NodeId> {
        let mut stack = vec![start];
        while let Some(x) = stack.pop() {
            if story.index_of(x).is_some() {
                return Some(x);
            }
            let kids: Vec<NodeId> = tree.node_ref(x).child_elements().map(|c| c.id()).collect();
            stack.extend(kids.into_iter().rev());
        }
        None
    };
    if let Some(found) = first_in(id) {
        return FrameChange::Before(found);
    }
    let mut at = tree.node_ref(id);
    loop {
        let mut next = at.next_sibling();
        while let Some(s) = next {
            if s.is_element()
                && let Some(found) = first_in(s.id())
            {
                return FrameChange::Before(found);
            }
            next = s.next_sibling();
        }
        match at.parent() {
            Some(p) => at = p,
            None => return FrameChange::Unowned,
        }
    }
}

/// The differences between the frame of `now` and that of `was`, the base's tree of the same part. Blocks added, removed or
/// moved are not frame differences: the guard reads them as blocks.
pub(super) fn frame_changes(story: &Story, now: &SpanTree, was: &SpanTree) -> Vec<FrameChange> {
    let mut out: Vec<FrameChange> = Vec::new();
    let root = now.root().id();
    let mut stack: Vec<NodeId> = vec![root];
    while let Some(id) = stack.pop() {
        let (el, old) = (now.node_ref(id), was.node_ref(id));
        if !old.is_element() {
            out.push(owner(story, now, id));
            continue;
        }
        if !el.is_dirty() {
            continue;
        }
        if el.local_name() != old.local_name()
            || el.namespace() != old.namespace()
            || attributes(now, id) != attributes(was, id)
        {
            out.push(if id == root || el.local_name() == "body" {
                FrameChange::Unowned
            } else {
                owner(story, now, id)
            });
        }
        let had: HashSet<NodeId> = old.children().map(|c| c.id()).collect();
        let has: HashSet<NodeId> = el.children().map(|c| c.id()).collect();
        for child in el.children().map(|c| c.id()) {
            if is_block(now, child) || is_space(now, child) {
                continue;
            }
            let node = now.node_ref(child);
            if !had.contains(&child) {
                out.push(owner(story, now, child));
            } else if node.is_dirty() {
                if node.is_element() {
                    stack.push(child);
                } else {
                    out.push(owner(story, now, child));
                }
            }
        }
        for gone in old.children().map(|c| c.id()) {
            if !has.contains(&gone) && !is_block(was, gone) && !is_space(was, gone) {
                out.push(owner(story, was, gone));
            }
        }
    }
    out
}
