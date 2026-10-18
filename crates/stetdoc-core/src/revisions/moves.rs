//! Which move a `w:moveFrom` or `w:moveTo` belongs to (design `revisions.md`, 3.5). The wrapper carries no
//! name: the range markers that surround it do, one `moveFromRangeStart` or `moveToRangeStart` before and the
//! `...RangeEnd` with the same id after, and a move over several paragraphs has one pair around all of
//! its wrappers. So the pairing is by position: a wrapper belongs to the nearest start before it of its side
//! whose end is after it.

use crate::wordml::{NodeId, Story};
use crate::xml::SpanTree;
use std::collections::{HashMap, HashSet};

/// The range of a move that a wrapper is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MoveRange {
    /// The `w:name` of the start, which the two sides of a move share.
    pub name: String,
    /// The start marker.
    pub start: NodeId,
    /// The end marker, if there is one.
    pub end: Option<NodeId>,
}

/// The pre-order position of each wanted node.
fn positions(tree: &SpanTree, wanted: &HashSet<NodeId>) -> HashMap<NodeId, u32> {
    let mut out = HashMap::new();
    let mut at = 0u32;
    let mut stack = vec![tree.root().children()];
    while let Some(top) = stack.last_mut() {
        let Some(n) = top.next() else {
            stack.pop();
            continue;
        };
        if !n.is_element() {
            continue;
        }
        at += 1;
        if wanted.contains(&n.id()) {
            out.insert(n.id(), at);
        }
        stack.push(n.children());
    }
    out
}

/// The move range of each of the wrappers, where it has one.
pub(super) fn ranges_of(
    story: &Story,
    tree: &SpanTree,
    wrappers: &[NodeId],
) -> HashMap<NodeId, MoveRange> {
    let mut out = HashMap::new();
    if wrappers.is_empty() {
        return out;
    }
    let anchors = story.anchors(tree).moves;
    let mut wanted: HashSet<NodeId> = wrappers.iter().copied().collect();
    for a in &anchors {
        wanted.extend(a.start.iter().map(|l| l.node));
        wanted.extend(a.end.iter().map(|l| l.node));
    }
    let pos = positions(tree, &wanted);
    for &w in wrappers {
        let (Some(&at), from) = (pos.get(&w), tree.node_ref(w).local_name() == "moveFrom") else {
            continue;
        };
        let best = anchors
            .iter()
            .filter(|a| a.from == from)
            .filter_map(|a| {
                let start = a.start.as_ref()?;
                let sp = *pos.get(&start.node)?;
                let ep = a.end.as_ref().and_then(|e| pos.get(&e.node).copied());
                (sp < at && ep.is_none_or(|e| e > at)).then_some((sp, a))
            })
            .max_by_key(|(sp, _)| *sp);
        if let Some((_, a)) = best
            && let Some(start) = &a.start
        {
            out.insert(
                w,
                MoveRange {
                    name: a.name.clone(),
                    start: start.node,
                    end: a.end.as_ref().map(|e| e.node),
                },
            );
        }
    }
    out
}
