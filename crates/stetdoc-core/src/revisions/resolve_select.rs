//! Which revisions a step selects (design `revisions.md`, 6.1). `All` takes every revision in every part
//! that can hold one. `Author` takes those of one author, and the empty author takes the ones with none.
//! `Ids` takes the whole change that holds each id, and `Elements`, which only the laws use, exactly the
//! elements with those ids.
//!
//! `All` and `Author` read every part. `Ids` and `Elements` do not: they find the elements through the
//! index, and `Ids` groups only the blocks that hold them (`changes` with a range), so a step costs what
//! its blocks cost and not what the document does.

use super::changes::{ChangeKind, changes};
use super::index::{ChangeIndex, Place, revision_parts};
use super::types::Selector;
use crate::api::error::StetError;
use crate::state::Tx;
use crate::wordml::{NodeId, RevKind, Revision, Story, part_revisions, revision_at};
use crate::xml::SpanTree;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// The revisions a step selects, by part, each in document order.
pub(super) type Selected = Vec<(Arc<str>, Vec<Revision>)>;

/// True for the revisions that are one side of a move.
fn is_move(k: RevKind) -> bool {
    matches!(
        k,
        RevKind::MoveFrom | RevKind::MoveTo | RevKind::MarkMoveFrom | RevKind::MarkMoveTo
    )
}

/// Selects the revisions of a step.
pub(super) fn select(
    tx: &Tx<'_>,
    index: &mut ChangeIndex,
    selector: &Selector,
) -> Result<Selected, StetError> {
    match selector {
        Selector::All => Ok(every_part(tx)?
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .collect()),
        Selector::Author(a) => by_author(tx, a),
        Selector::Ids(ids) => by_ids(tx, index, ids, true),
        #[cfg(test)]
        Selector::Elements(ids) => by_ids(tx, index, ids, false),
    }
}

/// Every revision of every part that can hold one, in document order.
fn every_part(tx: &Tx<'_>) -> Result<Selected, StetError> {
    let view = tx.parts();
    let mut all: Selected = Vec::new();
    for name in revision_parts(tx)? {
        let Some(part) = view.id(&name) else { continue };
        all.push((name.clone(), part_revisions(view.tree(part)?)));
    }
    Ok(all)
}

/// The revisions of one author, and the other side of every move among them.
fn by_author(tx: &Tx<'_>, author: &str) -> Result<Selected, StetError> {
    let all = every_part(tx)?;
    let mut chosen: HashSet<Place> = HashSet::new();
    for (name, revs) in &all {
        chosen.extend(
            revs.iter()
                .filter(|r| r.author.as_deref().unwrap_or("") == author)
                .map(|r| (name.clone(), r.node)),
        );
    }
    // Closure: both sides of a move.
    let moves: Vec<Place> = all
        .iter()
        .flat_map(|(name, revs)| {
            revs.iter()
                .filter(|r| is_move(r.kind))
                .map(|r| (name.clone(), r.node))
        })
        .filter(|p| chosen.contains(p))
        .collect();
    let view = tx.parts();
    for (part, node) in moves {
        let Some(id) = view.id(&part) else { continue };
        let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
            continue;
        };
        let owner = super::changes_text::owner_entry(&story, &tree.node_ref(node));
        for c in changes(&story, tree, Some(owner..owner + 1)) {
            if c.kind == ChangeKind::Move && c.nodes.contains(&node) {
                chosen.extend(c.nodes.iter().map(|n| (part.clone(), *n)));
            }
        }
    }
    Ok(all
        .into_iter()
        .map(|(name, revs)| {
            let picked: Vec<Revision> = revs
                .into_iter()
                .filter(|r| chosen.contains(&(name.clone(), r.node)))
                .collect();
            (name, picked)
        })
        .filter(|(_, v)| !v.is_empty())
        .collect())
}

/// Where a node is in document order: the entry that holds it and the way down from that entry, by
/// positions among the siblings. For a part with no entries, the way down from the root.
fn order_key(story: Option<&Story>, tree: &SpanTree, node: NodeId) -> (usize, Vec<u32>) {
    let mut path: Vec<u32> = Vec::new();
    let mut owner = usize::MAX;
    let mut cur = tree.node_ref(node);
    loop {
        if let Some(i) = story.and_then(|s| s.index_of(cur.id())) {
            owner = i;
            break;
        }
        let mut at = 0u32;
        let mut prev = cur.prev_sibling();
        while let Some(p) = prev {
            at = at.saturating_add(1);
            prev = p.prev_sibling();
        }
        path.push(at);
        let Some(parent) = cur.parent() else { break };
        cur = parent;
    }
    path.reverse();
    (owner, path)
}

/// The elements with these ids, found through the index, and for `Ids` the whole change that holds each.
fn by_ids(
    tx: &Tx<'_>,
    index: &mut ChangeIndex,
    ids: &[i64],
    whole_changes: bool,
) -> Result<Selected, StetError> {
    let mut chosen: BTreeMap<Arc<str>, Vec<NodeId>> = BTreeMap::new();
    for id in ids {
        let places = index.lookup(tx, *id)?;
        if places.is_empty() {
            return Err(super::errors::not_found(format!(
                "there is no revision with id {id}. Read the revisions again for the ids that exist."
            )));
        }
        for (part, node) in places {
            chosen.entry(part).or_default().push(node);
        }
    }
    let view = tx.parts();
    let mut out: Selected = Vec::new();
    for (part, mut nodes) in chosen {
        let Some(id) = view.id(&part) else { continue };
        let tree = view.tree(id)?;
        let story = view.story(id).ok();
        if let (true, Some(story)) = (whole_changes, story.as_deref()) {
            // The change that holds a node is found among the changes of the block that holds it.
            let mut by_block: HashMap<usize, Vec<super::changes::Change>> = HashMap::new();
            let mut more: Vec<NodeId> = Vec::new();
            for &node in &nodes {
                let owner = super::changes_text::owner_entry(story, &tree.node_ref(node));
                let found = by_block
                    .entry(owner)
                    .or_insert_with(|| changes(story, tree, Some(owner..owner + 1)));
                if let Some(c) = found.iter().find(|c| c.nodes.contains(&node)) {
                    more.extend(c.nodes.iter().copied());
                }
            }
            nodes.extend(more);
        }
        let mut seen: HashSet<NodeId> = HashSet::new();
        nodes.retain(|n| seen.insert(*n));
        let mut keyed: Vec<((usize, Vec<u32>), Revision)> = nodes
            .into_iter()
            .filter_map(|n| {
                let r = revision_at(&tree.node_ref(n))?;
                Some((order_key(story.as_deref(), tree, n), r))
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        if !keyed.is_empty() {
            out.push((part, keyed.into_iter().map(|(_, r)| r).collect()));
        }
    }
    Ok(out)
}
