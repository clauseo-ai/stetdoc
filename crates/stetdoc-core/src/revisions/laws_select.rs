//! Laws for reading changes, which hold on every state: a window of entries gives what the whole story
//! gives for it, and a step by id takes the whole change that holds the id, the same changes that the
//! whole story groups.

use super::changes::{Change, changes};
use super::index::{ChangeIndex, revision_parts};
use super::resolve_select::select;
use super::types::Selector;
use crate::state::State;
use crate::wordml::{NodeId, Story, revision_at};
use crate::xml::SpanTree;
use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;

/// The entries to look at in a story: an even sample of its blocks, and the first, the middle and the last
/// entry that each of an even sample of its changes touches.
fn positions(n: usize, all: &[Change]) -> Vec<usize> {
    let mut at: Vec<usize> = (0..n).step_by((n / 40).max(1)).collect();
    for c in all.iter().step_by((all.len() / 60).max(1)) {
        at.extend(c.touches.first().copied());
        at.extend(c.touches.get(c.touches.len() / 2).copied());
        at.extend(c.touches.last().copied());
    }
    at.sort_unstable();
    at.dedup();
    at.retain(|&i| i < n);
    at
}

/// What the whole story gives for a range: the changes that touch it.
fn overlapping(all: &[Change], range: &Range<usize>) -> Vec<Change> {
    all.iter()
        .filter(|c| c.touches.iter().any(|i| range.contains(i)))
        .cloned()
        .collect()
}

/// The story parts of a state, by name.
fn story_parts(state: &State) -> Vec<Arc<str>> {
    let tx = state.begin();
    let view = tx.parts();
    revision_parts(&tx)
        .unwrap_or_default()
        .into_iter()
        .filter(|name| {
            view.id(name)
                .is_some_and(|id| view.story(id).is_ok() && view.tree(id).is_ok())
        })
        .collect()
}

fn with_story<T>(
    state: &State,
    name: &str,
    f: impl FnOnce(&Story, &SpanTree) -> Result<T, String>,
) -> Result<Option<T>, String> {
    let view = state.parts();
    let Some(id) = view.id(name) else {
        return Ok(None);
    };
    let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
        return Ok(None);
    };
    f(&story, tree).map(Some)
}

/// R10: `changes(range)` is the whole story's changes that touch the range, for a range of one entry and a
/// range of three, in every story part. Returns what is wrong, if anything.
pub(super) fn check_windows(state: &State) -> Result<(), String> {
    for name in story_parts(state) {
        with_story(state, &name, |story, tree| {
            let n = story.blocks().len();
            let all = changes(story, tree, None);
            for i in positions(n, &all) {
                for range in [i..i + 1, i..(i + 3).min(n)] {
                    let got = changes(story, tree, Some(range.clone()));
                    let want = overlapping(&all, &range);
                    if got != want {
                        let show = |v: &[Change]| {
                            v.iter()
                                .map(|c| {
                                    format!("{:?} {:?} touches {:?}", c.kind, c.ids, c.touches)
                                })
                                .collect::<Vec<_>>()
                        };
                        return Err(format!(
                            "R10 broken in {name}: changes({range:?}) gives {:?}, the whole story gives {:?} for it",
                            show(&got),
                            show(&want)
                        ));
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}

/// R11: `Ids` selects every element of every change that holds an element with the id, as the whole
/// story groups them, and `Elements` selects the elements with the id and nothing else.
pub(super) fn check_ids(state: &State) -> Result<(), String> {
    let tx = state.begin();
    let mut index = ChangeIndex::new();
    for name in story_parts(state) {
        with_story(state, &name, |story, tree| {
            let all = changes(story, tree, None);
            let id_of = |node: NodeId| revision_at(&tree.node_ref(node)).and_then(|r| r.id);
            for change in all.iter().step_by((all.len() / 12).max(1)) {
                for &wanted in &change.ids {
                    let mut whole: HashSet<NodeId> = HashSet::new();
                    let mut exact: HashSet<NodeId> = HashSet::new();
                    for c in &all {
                        if c.nodes.iter().any(|&n| id_of(n) == Some(wanted)) {
                            whole.extend(c.nodes.iter().copied());
                        }
                        exact.extend(
                            c.nodes
                                .iter()
                                .copied()
                                .filter(|&n| id_of(n) == Some(wanted)),
                        );
                    }
                    for (selector, want, label) in [
                        (Selector::Ids(vec![wanted]), &whole, "Ids"),
                        (Selector::Elements(vec![wanted]), &exact, "Elements"),
                    ] {
                        let got: HashSet<NodeId> = select(&tx, &mut index, &selector)
                            .map_err(|e| format!("R11: select {label} {wanted} failed: {e}"))?
                            .iter()
                            .filter(|(part, _)| **part == *name)
                            .flat_map(|(_, revs)| revs.iter().map(|r| r.node))
                            .collect();
                        if &got != want {
                            return Err(format!(
                                "R11 broken in {name}: {label}({wanted}) selects nodes {got:?}, the whole story's changes say {want:?}"
                            ));
                        }
                    }
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
