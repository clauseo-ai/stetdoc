//! `ChangeIndex`: the revision elements of a batch by `w:id` (design `revisions.md`, 6.1).
//!
//! It lives for one batch. It is empty until the first `resolve` that names ids, which builds it by one
//! walk of the `Tx`'s current trees (the batch may already have written revisions that only the staged
//! trees hold). After that it is kept up to date and never rebuilt: after each compiler call `note` adds
//! the revision elements of the blocks that call touched or made (the call's own list, because a block
//! that an earlier call touched can get a new revision from this one) and drops the ids it removed, and a
//! lookup checks what it finds, so an element that a later step removed is dropped when it is asked for.

use super::cleanup::is_attached;
use super::outcome::Effect;
use crate::api::error::StetError;
use crate::state::Tx;
use crate::wordml::{NodeId, part_revisions, revision_at};
use std::collections::HashMap;
use std::sync::Arc;

/// Where a revision element is: its part and its node.
pub(crate) type Place = (Arc<str>, NodeId);

/// The revision elements of one batch, by id.
#[derive(Debug, Default)]
pub(crate) struct ChangeIndex {
    /// `None` until built.
    map: Option<HashMap<i64, Vec<Place>>>,
}

/// The names of the parts that can hold revisions: the stories, and the styles and numbering parts.
pub(super) fn revision_parts(tx: &Tx<'_>) -> Result<Vec<Arc<str>>, StetError> {
    let view = tx.parts();
    let specs = view.story_specs()?;
    let mut out: Vec<Arc<str>> = specs.0.iter().map(|s| s.part.clone()).collect();
    if let Some(main) = out.first().and_then(|n| view.id(n)) {
        for suffix in ["/relationships/styles", "/relationships/numbering"] {
            if let Some(id) = view.related_part(main, &|t| t.ends_with(suffix))?
                && let Some(name) = view.name(id)
                && !out.iter().any(|o| o == name)
            {
                out.push(name.clone());
            }
        }
    }
    Ok(out)
}

impl ChangeIndex {
    /// An empty index.
    pub(crate) fn new() -> ChangeIndex {
        ChangeIndex::default()
    }

    /// Notes what a compiler call did: the revisions in the blocks it touched or made are added, and the
    /// ids it removed are dropped.
    pub(crate) fn note(&mut self, tx: &Tx<'_>, effect: &Effect) {
        let Some(map) = self.map.as_mut() else {
            return;
        };
        for id in effect
            .removed_own
            .iter()
            .filter_map(|r| r.id)
            .chain(effect.swallowed.iter().copied())
        {
            map.remove(&id);
        }
        let view = tx.parts();
        for key in effect.touched.iter().chain(effect.new_blocks.iter()) {
            let Some(part) = view.id(key.part()) else {
                continue;
            };
            let Ok(tree) = view.tree(part) else { continue };
            if !is_attached(tree, key.node()) {
                continue;
            }
            let mut stack = vec![tree.node_ref(key.node())];
            while let Some(n) = stack.pop() {
                if let Some(r) = revision_at(&n)
                    && let Some(id) = r.id
                {
                    let entry = map.entry(id).or_default();
                    let place = (key.part().clone(), n.id());
                    if !entry.contains(&place) {
                        entry.push(place);
                    }
                }
                stack.extend(n.child_elements());
            }
        }
    }

    /// The elements with this id that are still in the document. The first call builds the index.
    pub(crate) fn lookup(&mut self, tx: &Tx<'_>, id: i64) -> Result<Vec<Place>, StetError> {
        if self.map.is_none() {
            let mut map: HashMap<i64, Vec<Place>> = HashMap::new();
            let view = tx.parts();
            for name in revision_parts(tx)? {
                let Some(part) = view.id(&name) else { continue };
                let tree = view.tree(part)?;
                for r in part_revisions(tree) {
                    if let Some(id) = r.id {
                        map.entry(id).or_default().push((name.clone(), r.node));
                    }
                }
            }
            self.map = Some(map);
        }
        let view = tx.parts();
        let Some(list) = self.map.as_mut().and_then(|m| m.get_mut(&id)) else {
            return Ok(Vec::new());
        };
        list.retain(|(part, node)| {
            view.id(part)
                .and_then(|p| view.tree(p).ok())
                .is_some_and(|tree| {
                    is_attached(tree, *node)
                        && revision_at(&tree.node_ref(*node)).and_then(|r| r.id) == Some(id)
                })
        });
        Ok(list.clone())
    }
}
