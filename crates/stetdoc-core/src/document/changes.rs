//! Revisions as the public interface shows them: the ledger of `projection` turned into `ChangeEntry`, which revision
//! belongs to which operation, and `Document::revisions` (design `document-and-ops.md` 4.2 and 6). The grouping of the
//! changes is `revisions::changes`'s, and the description and the filter are the ledger's.

use super::Document;
use crate::api::error::StetError;
use crate::api::receipt::ChangeEntry;
use crate::api::revision::RevisionInfo;
use crate::api::view::RevisionFilter;
use crate::ops::Applied;
use crate::projection::{LedgerFilter, changes_of, describe};
use crate::revisions::changes::changes;
use crate::state::PartsView;
use crate::wordml::{BlockKey, RevKind};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

/// The family of one revision element, for the counts of a resolution: `insertion`, `deletion`, `move` or `format`.
pub(super) fn element_family(kind: RevKind) -> &'static str {
    match kind {
        RevKind::Ins
        | RevKind::MarkIns
        | RevKind::RowIns
        | RevKind::CellIns
        | RevKind::NumPrIns
        | RevKind::CustomXmlInsStart
        | RevKind::CustomXmlInsEnd => "insertion",
        RevKind::Del
        | RevKind::MarkDel
        | RevKind::RowDel
        | RevKind::CellDel
        | RevKind::CustomXmlDelStart
        | RevKind::CustomXmlDelEnd => "deletion",
        RevKind::MoveFrom
        | RevKind::MoveTo
        | RevKind::MarkMoveFrom
        | RevKind::MarkMoveTo
        | RevKind::CustomXmlMoveFromStart
        | RevKind::CustomXmlMoveFromEnd
        | RevKind::CustomXmlMoveToStart
        | RevKind::CustomXmlMoveToEnd => "move",
        _ => "format",
    }
}

/// How a `ChangeEntry` shows a change.
pub(super) fn entry(info: &RevisionInfo) -> ChangeEntry {
    ChangeEntry {
        kind: info.kind,
        ids: info.ids.clone(),
        date: info.date.clone().unwrap_or_default(),
        block: info.block.clone(),
    }
}

/// A change that the batch wrote, with the first operation that minted one of its ids.
pub(super) struct Written {
    /// The change.
    pub info: RevisionInfo,
    /// The operation that wrote it. An insertion that coalesced into the revision of an earlier operation is listed there.
    pub op: Option<usize>,
}

/// The changes that the batch wrote: the changes in the touched and new blocks that hold a revision the transaction created.
pub(super) fn written(parts: &PartsView<'_>, applied: &Applied) -> Result<Vec<Written>, StetError> {
    let created: HashSet<i64> = applied
        .outcomes
        .iter()
        .flat_map(|o| o.created.iter().map(|c| c.id))
        .collect();
    if created.is_empty() {
        return Ok(Vec::new());
    }
    // The entries of each story that the batch touched or made.
    let mut wanted: BTreeMap<Arc<str>, Vec<usize>> = BTreeMap::new();
    let mut seen: HashSet<BlockKey> = HashSet::new();
    for op in &applied.ops {
        for effect in &op.effects {
            for key in effect.touched.iter().chain(effect.new_blocks.iter()) {
                if !seen.insert(key.clone()) || !parts.is_attached(key)? {
                    continue;
                }
                let Some(part) = parts.id(key.part()) else {
                    continue;
                };
                // A header or a footer that no section names any more after a later operation of the batch holds no change
                // the receipt can show: the commit retires its blocks' refs.
                if !parts.is_story(part)? {
                    continue;
                }
                if let Some(index) = parts.story(part)?.index_of(key.node()) {
                    wanted.entry(key.part().clone()).or_default().push(index);
                }
            }
        }
    }
    // The first operation that minted each id, so that finding the operation of a change does not search every effect of
    // the batch for every change it wrote.
    let mut minted_by: HashMap<u32, usize> = HashMap::new();
    for (i, op) in applied.ops.iter().enumerate() {
        for e in &op.effects {
            for id in e.minted.clone() {
                minted_by.entry(id).or_insert(i);
            }
        }
    }
    let mut out: Vec<Written> = Vec::new();
    let mut counted: HashSet<i64> = HashSet::new();
    for spec in &parts.story_specs()?.0 {
        let Some(indexes) = wanted.get_mut(&spec.part) else {
            continue;
        };
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        let story = parts.story(part)?;
        let tree = parts.tree(part)?;
        indexes.sort_unstable();
        indexes.dedup();
        // Neighbouring entries are one window: a change spans consecutive blocks.
        let mut windows: Vec<Range<usize>> = Vec::new();
        for &i in indexes.iter() {
            match windows.last_mut() {
                Some(w) if i <= w.end + 2 => w.end = i + 1,
                _ => windows.push(i..i + 1),
            }
        }
        for window in windows {
            for change in changes(&story, tree, Some(window)) {
                let first = change.ids.first().copied();
                if !change.ids.iter().any(|id| created.contains(id)) {
                    continue;
                }
                if first.is_some_and(|id| !counted.insert(id)) {
                    continue;
                }
                let op = change
                    .ids
                    .iter()
                    .filter_map(|id| u32::try_from(*id).ok())
                    .filter_map(|n| minted_by.get(&n).copied())
                    .min();
                out.push(Written {
                    info: describe(&change, parts, &story, spec.kind),
                    op,
                });
            }
        }
    }
    Ok(out)
}

impl Document {
    /// Every change in the document that matches the filter, in document order: what `check` lists for a file with nothing
    /// pending, and what a citation gate reads. `author` is a name or the short code the view shows.
    pub fn revisions(&self, filter: &RevisionFilter) -> Result<Vec<RevisionInfo>, StetError> {
        let parts = self.state.parts();
        let keep = LedgerFilter::of(parts, filter)?;
        Ok(changes_of(parts, &keep)?
            .into_iter()
            .filter_map(|listed| listed.shown)
            .collect())
    }
}
