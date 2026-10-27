//! The `Revisions` scope with context: the blocks that hold the kept changes, whole, with their neighbours, so that a
//! reviewer reads each change in its paragraph in one view.

use super::ledger::{LedgerFilter, changes_of};
use super::select::{Item, Selected, Source, locate};
use crate::api::error::StetError;
use crate::api::view::{Mode, RevisionFilter};
use crate::state::{PartId, PartsView, State};

/// The tops that hold the first block of a change `filter` keeps, with `filter.context` tops on each side, in story
/// order. Each item counts the changes it holds in `matches`. The label is the ledger's, with the changes and the blocks
/// counted.
pub(crate) fn changes_scope(
    parts: &PartsView<'_>,
    state: &State,
    mode: Mode,
    filter: &RevisionFilter,
    label: String,
) -> Result<Selected, StetError> {
    let keep = LedgerFilter::of(*parts, filter)?;
    let all = changes_of(*parts, &keep)?;
    let mut places: Vec<(PartId, usize)> = Vec::new();
    for listed in &all {
        if let Some(block) = listed.shown.as_ref().and_then(|s| s.block.as_deref()) {
            places.push(locate(parts, state, block)?);
        }
    }
    let changes = places.len();
    let reach = usize::from(filter.context);
    let mut sources = Vec::new();
    let mut items = Vec::new();
    let mut blocks = 0usize;
    for spec in &parts.story_specs()?.0 {
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        if !places.iter().any(|(p, _)| *p == part) {
            continue;
        }
        let source = Source::load(parts, part, mode, true)?;
        let mut tops: Vec<(usize, u32)> = Vec::new();
        for (_, entry) in places.iter().filter(|(p, _)| *p == part) {
            let Some(top) = source.top_of(*entry) else {
                continue;
            };
            match tops.iter_mut().find(|(t, _)| *t == top) {
                Some((_, n)) => *n = n.saturating_add(1),
                None => tops.push((top, 1)),
            }
        }
        tops.sort_unstable();
        blocks += tops.len();
        let src = sources.len();
        let mut next = 0usize;
        for (t, _) in &tops {
            let from = t.saturating_sub(reach).max(next);
            let to = (t + reach + 1).min(source.tops.len());
            for top in from..to {
                let matches = tops.iter().find(|(h, _)| *h == top).map_or(0, |(_, n)| *n);
                items.push(Item { src, top, matches });
            }
            next = to.max(next);
        }
        sources.push(source);
    }
    let counted = match (changes, blocks) {
        (0, _) => "no changes".to_string(),
        (1, _) => "1 change in 1 block".to_string(),
        (n, 1) => format!("{n} changes in 1 block"),
        (n, b) => format!("{n} changes in {b} blocks"),
    };
    Ok(Selected {
        name: "changes",
        label: Some(format!("{label}: {counted}")),
        sources,
        items,
        focus: Vec::new(),
    })
}
