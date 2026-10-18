//! `Story::refresh`: rebuild the story of a tree that was edited since the previous scan.
//!
//! A refresh must give exactly the value a scan from scratch gives, and the coherence test compares
//! them after every edit. The block scan is run again in full (under 2 ms at 13 MB). What a refresh
//! may reuse is the field index the story caches for itself, because building it walks every
//! paragraph. When the edit did not change field structure the index is shared with the previous
//! story as it is. When it did, the pass runs again from what the previous index knows: it looks
//! only at the paragraphs that held field markup and at the ones that were edited, and not at the
//! rest of the story.
//!
//! **`touched` is not trusted for the shape of the story.** The new scan is compared with the previous
//! block list by `NodeId`, and every new, removed and moved block counts as touched. So `touched`
//! only has to cover edits inside existing blocks, which a comparison of a block's bytes before and
//! after can check; a new block has no before, a removed block no after, and a moved block the same
//! bytes. The scan also keeps a key of the wrappers above each block, so a block that was wrapped
//! into or out of a control or a revision, or whose row or cell gained a revision, is reported as
//! `context_changed`: its text changed in some mode and its bytes and place did not.
//! `docs/design/wordml.md` 3.3 and 4.4.

use super::names::is_w;
use super::story::{BlockKind, Story};
use super::text_fields::{FieldIndex, ParaState, Phase, Reuse, has_field_markup};
use crate::api::error::StetError;
use crate::xml::{NodeId, NodeKind, SpanTree};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The result of [`Story::refresh`]: the new story, and what the comparison with the previous one found.
#[derive(Clone, Debug)]
pub struct Refreshed {
    /// The story of the new tree. It equals a scan from scratch.
    pub story: Story,
    /// Blocks in the new story that the previous one did not have.
    pub added: Vec<NodeId>,
    /// Blocks of the previous story that the new one does not have.
    pub removed: Vec<NodeId>,
    /// Blocks in both that left the longest run of blocks that kept their order, or changed parent.
    pub moved: Vec<NodeId>,
    /// Blocks in both whose wrappers changed: it is now inside a control, a revision, a row or a
    /// cell marked by a revision that it was not inside, or the other way round, or the revision
    /// of its row or cell changed. What the block reads in each mode changed, and neither its bytes
    /// nor its place did. A block that was also moved can be here as well.
    pub context_changed: Vec<NodeId>,
    /// Paragraphs of the new story that start in a different field state than they did, a new
    /// paragraph counting as having started in none. Their text changes without their XML changing.
    pub field_starts_changed: Vec<NodeId>,
    /// True if the field index of the previous story was shared with the new one as it was. False if
    /// the previous story had none (nothing to share), or if the edit changed field structure and the
    /// pass ran again, once, inside the refresh.
    pub fields_carried: bool,
}

impl Story {
    /// The story of `tree`, which was edited since `prev` was scanned. `touched` lists the blocks
    /// whose content the edit changed. It need not list blocks that are new, removed or moved.
    pub fn refresh(
        prev: &Story,
        tree: &SpanTree,
        touched: &[NodeId],
    ) -> Result<Refreshed, StetError> {
        let story = Story::scan(&prev.part, prev.kind, tree)?;
        let diff = Diff::of(prev, &story);
        let mut field_starts_changed = Vec::new();
        let mut fields_carried = false;
        if let Some(old) = prev.fields.get() {
            if can_share(old, &story, tree, touched, &diff) {
                let _ = story.fields.set(Arc::clone(old));
                fields_carried = true;
            } else {
                let mut dirty: HashSet<NodeId> = touched.iter().copied().collect();
                dirty.extend(diff.added.iter().chain(&diff.moved).copied());
                // the pass runs here, once, because the paragraphs whose start changed are found by
                // comparing the new index with the old one
                let new = Arc::new(FieldIndex::build(
                    &story,
                    tree,
                    Some(Reuse { old, dirty: &dirty }),
                ));
                field_starts_changed = start_changes(old, &new, &story);
                let _ = story.fields.set(new);
            }
        }
        Ok(Refreshed {
            story,
            added: diff.added,
            removed: diff.removed,
            moved: diff.moved,
            context_changed: diff.context_changed,
            field_starts_changed,
            fields_carried,
        })
    }
}

/// How the block list changed.
#[derive(Default)]
struct Diff {
    added: Vec<NodeId>,
    removed: Vec<NodeId>,
    moved: Vec<NodeId>,
    context_changed: Vec<NodeId>,
    /// The new indexes of the added and the moved blocks, in order.
    shape: Vec<usize>,
}

/// The element of the parent entry of entry `i`.
fn parent_node(s: &Story, i: usize) -> Option<NodeId> {
    s.entries
        .get(i)
        .and_then(|e| e.parent)
        .and_then(|p| s.entries.get(p as usize))
        .map(|p| p.node)
}

/// The element of the note or comment that entry `i` is in, if it is in one. It is the identity of
/// the unit, where the entry's own `unit` is an index that moves when another unit is added or
/// removed.
fn unit_node(s: &Story, i: usize) -> Option<NodeId> {
    s.entries
        .get(i)
        .and_then(|e| s.units.get(e.unit as usize))
        .map(|u| u.node)
}

impl Diff {
    /// Compares two block lists. A run of blocks at the start and one at the end that are the same
    /// elements in the same places with the same parents and the same note or comment are matched by
    /// position, which is all that is left to do for most edits, and only what lies between them is
    /// matched through maps. A block that went to another parent or another unit is moved, since
    /// where it is changes what it reads, such as the fields open where it starts.
    fn of(prev: &Story, new: &Story) -> Diff {
        let (a, b) = (&prev.entries, &new.entries);
        let place = |i: usize, j: usize| {
            parent_node(prev, i) == parent_node(new, j) && unit_node(prev, i) == unit_node(new, j)
        };
        let aligned = |i: usize, j: usize| match (a.get(i), b.get(j)) {
            (Some(x), Some(y)) => x.node == y.node && place(i, j),
            _ => false,
        };
        let limit = a.len().min(b.len());
        let mut head = 0;
        while head < limit && aligned(head, head) {
            head += 1;
        }
        let mut tail = 0;
        while tail < limit - head && aligned(a.len() - 1 - tail, b.len() - 1 - tail) {
            tail += 1;
        }
        let mut out = Diff::default();
        // new indexes of the blocks whose wrappers changed, so they come out in document order
        let mut changed: Vec<usize> = Vec::new();
        let differs = |i: usize, j: usize| prev.ctx.get(i) != new.ctx.get(j);
        for i in 0..head {
            if differs(i, i) {
                changed.push(i);
            }
        }
        for k in 0..tail {
            let (i, j) = (a.len() - tail + k, b.len() - tail + k);
            if differs(i, j) {
                changed.push(j);
            }
        }

        let window_a = head..a.len() - tail;
        let window_b = head..b.len() - tail;
        let in_prev: HashMap<NodeId, usize> = window_a
            .clone()
            .filter_map(|i| Some((a.get(i)?.node, i)))
            .collect();
        let in_new: HashSet<NodeId> = window_b
            .clone()
            .filter_map(|j| Some(b.get(j)?.node))
            .collect();
        out.removed = window_a
            .filter_map(|i| a.get(i))
            .filter(|e| !in_new.contains(&e.node))
            .map(|e| e.node)
            .collect();
        // the previous index of each block both stories have, with its new index, in the new order
        let mut common: Vec<(usize, usize)> = Vec::new();
        for j in window_b {
            let Some(e) = b.get(j) else { continue };
            match in_prev.get(&e.node) {
                Some(&i) => common.push((j, i)),
                None => {
                    out.added.push(e.node);
                    out.shape.push(j);
                }
            }
        }
        let order: Vec<usize> = common.iter().map(|c| c.1).collect();
        let kept = longest_increasing(&order);
        for (&(j, i), kept) in common.iter().zip(kept) {
            let Some(e) = b.get(j) else { continue };
            if !kept || !place(i, j) {
                out.moved.push(e.node);
                out.shape.push(j);
            }
            if differs(i, j) {
                changed.push(j);
            }
        }
        changed.sort_unstable();
        out.shape.sort_unstable();
        out.context_changed = changed
            .into_iter()
            .filter_map(|j| b.get(j))
            .map(|e| e.node)
            .collect();
        out
    }
}

/// Marks the elements of a longest strictly increasing run of `seq`.
fn longest_increasing(seq: &[usize]) -> Vec<bool> {
    let mut tails: Vec<usize> = Vec::new();
    let mut link: Vec<Option<usize>> = vec![None; seq.len()];
    for (i, &v) in seq.iter().enumerate() {
        let at = tails.partition_point(|&t| seq.get(t).is_some_and(|&x| x < v));
        if at > 0
            && let Some(slot) = link.get_mut(i)
        {
            *slot = tails.get(at - 1).copied();
        }
        match tails.get_mut(at) {
            Some(t) => *t = i,
            None => tails.push(i),
        }
    }
    let mut kept = vec![false; seq.len()];
    let mut cur = tails.last().copied();
    while let Some(i) = cur {
        if let Some(k) = kept.get_mut(i) {
            *k = true;
        }
        cur = link.get(i).copied().flatten();
    }
    kept
}

/// True if `id` is still in the tree: its parents lead up to the document node.
fn attached(tree: &SpanTree, id: NodeId) -> bool {
    let mut cur = tree.node_ref(id);
    for _ in 0..100_000 {
        if cur.kind() == NodeKind::Document {
            return true;
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return false,
        }
    }
    false
}

/// True if every part of every field, its `begin`, `separate` and `end`, is still in the tree. A field
/// character that was detached from a paragraph nobody listed as touched is noticed here. Only a
/// paragraph that was edited can have lost one, and the span tree knows which have been (a node is
/// dirty if it or anything below it was changed), so only the parts in those paragraphs are followed
/// up to the document, which is a few reads of memory for each, and the others cost a lookup. The
/// `begin` and the `separate` are in the paragraph that holds the start, since field code cannot cross
/// a paragraph (see `Builder::finish` in `text_build.rs`), and the `end` is in the paragraph that holds
/// the end.
fn parts_in_place(old: &FieldIndex, tree: &SpanTree) -> bool {
    let edited: HashSet<NodeId> = old
        .marked
        .iter()
        .filter(|n| tree.node_ref(**n).is_dirty())
        .copied()
        .collect();
    if edited.is_empty() {
        return true;
    }
    let lost = |paragraph: Option<NodeId>, parts: &[Option<NodeId>]| {
        paragraph.is_some_and(|p| edited.contains(&p))
            && parts.iter().flatten().any(|&n| !attached(tree, n))
    };
    old.fields.iter().all(|f| {
        !lost(Some(f.paragraph_begin), &[Some(f.begin), f.separate])
            && !lost(f.paragraph_end, &[f.end])
    })
}

/// True if the field index of the previous story is also the index of the new one, as it is.
///
/// That holds when every block that was added or moved leaves the fields alone (see
/// [`leaves_fields_alone`]); no removed block held field markup or sat in an open field (a removed
/// paragraph is not in the new scan, but it is still in the tree if it was moved where the scan does
/// not read, and a stray `w:instrText` in it counted); no paragraph that was edited held field markup
/// before or holds any now; and every part of every field is still in the tree. Each of these is a
/// lookup in the index or a walk of one edited paragraph.
fn can_share(
    old: &FieldIndex,
    story: &Story,
    tree: &SpanTree,
    touched: &[NodeId],
    diff: &Diff,
) -> bool {
    let held = |n: &NodeId| old.marked.contains(n) || old.paras.contains_key(n);
    if diff.removed.iter().any(held) || !leaves_fields_alone(old, story, tree, &diff.shape) {
        return false;
    }
    if !parts_in_place(old, tree) {
        return false;
    }
    touched.iter().all(|n| {
        let node = tree.node_ref(*n);
        // only a paragraph can hold field markup; a table or a row that was touched owns its
        // properties and its list, and the paragraphs in it are listed on their own
        !is_w(&node, "p") || (!old.marked.contains(n) && !has_field_markup(&node))
    })
}

/// True if the blocks at these entries of the new story, which were added or moved, change nothing
/// the field index holds. A paragraph does not if it holds no field markup now and held none (or
/// state) before, and the fields open where it starts are none, as for a paragraph written into a
/// document outside every field, which is what an agent does. Then the paragraphs around it keep the
/// state they had, and it takes none. One that goes into an open field gets an entry in the index,
/// and one that holds markup changes it: the pass runs again for those. A table or a row holds no
/// state of its own, and the paragraphs in it are looked at on their own.
fn leaves_fields_alone(old: &FieldIndex, story: &Story, tree: &SpanTree, shape: &[usize]) -> bool {
    // the verdict on the fields open before the last entry looked at, which holds for the entry after
    // it when no entry is between them, since a block that leaves the fields alone passes them on
    let mut before: Option<(usize, bool)> = None;
    for &j in shape {
        let Some(e) = story.entries.get(j) else {
            return false;
        };
        let outside = match before {
            Some((last, outside)) if last + 1 == j => outside,
            _ => none_open_before(old, story, shape, j),
        };
        before = Some((j, outside));
        if e.kind != BlockKind::Paragraph {
            continue;
        }
        let node = tree.node_ref(e.node);
        let held = old.marked.contains(&e.node) || old.paras.contains_key(&e.node);
        if !outside || held || has_field_markup(&node) {
            return false;
        }
    }
    true
}

/// True if no field is open where entry `j` of the new story starts: the entry before it that is a
/// paragraph of the same note or comment, and that was not added or moved, ended with none open, or
/// there is no such entry and a unit starts afresh. A paragraph with no entry in the index ended with
/// none open.
fn none_open_before(old: &FieldIndex, story: &Story, shape: &[usize], j: usize) -> bool {
    let Some(unit) = story.entries.get(j).map(|e| e.unit) else {
        return false;
    };
    for i in (0..j).rev() {
        let Some(q) = story.entries.get(i) else {
            return false;
        };
        if q.unit != unit {
            return true;
        }
        if q.kind == BlockKind::Paragraph && shape.binary_search(&i).is_err() {
            return old
                .paras
                .get(&q.node)
                .is_none_or(|state| state.end.is_empty());
        }
    }
    true
}

/// Whether two paragraph states start with the same open fields, in the same phase.
fn same_start(a: Option<&ParaState>, b: Option<&ParaState>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), None) | (None, Some(x)) => x.start.is_empty(),
        (Some(x), Some(y)) => {
            x.start.len() == y.start.len()
                && x.start
                    .iter()
                    .zip(&y.start)
                    .all(|(p, q)| p.begin == q.begin && p.phase == q.phase)
        }
    }
}

/// The paragraphs of the new story that start in a different field state than they did. Only a
/// paragraph with an entry in either index can differ, so the entries are the candidates.
fn start_changes(old: &FieldIndex, new: &FieldIndex, story: &Story) -> Vec<NodeId> {
    if old.paras.is_empty() && new.paras.is_empty() {
        return Vec::new();
    }
    let changed: HashSet<NodeId> = old
        .paras
        .keys()
        .chain(new.paras.keys())
        .filter(|n| !same_start(old.paras.get(n), new.paras.get(n)))
        .copied()
        .collect();
    story
        .entries
        .iter()
        .map(|e| e.node)
        .filter(|n| changed.contains(n))
        .collect()
}
