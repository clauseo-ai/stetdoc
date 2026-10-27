//! The `Find` scope: the blocks whose text holds a needle, with their neighbours, in every story or within one range.

use super::select::{Item, Selected, Source, count, error, locate};
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::Mode;
use crate::state::{PartId, PartsView, State};
use crate::wordml::BlockKind;

/// The tops that hold the needle, with `context` tops on each side, in every story, or only in the range `within`:
/// from a ref to a ref, or to the end of the story of the first. The label counts the matches and the blocks that hold
/// them.
pub(crate) fn find_scope(
    parts: &PartsView<'_>,
    state: &State,
    mode: Mode,
    needle: &str,
    context: u8,
    within: Option<(&str, Option<&str>)>,
) -> Result<Selected, StetError> {
    if needle.trim().is_empty() {
        return Err(error(
            ErrorCode::InvalidPayload,
            "a find needs some text to look for.",
            "Give the text to find.",
        ));
    }
    let fold = !needle.chars().any(char::is_uppercase);
    let lowered = needle.to_lowercase();
    let wanted = if fold { lowered.as_str() } else { needle };
    let limit = match within {
        Some((from, to)) => Some(find_limit(parts, state, from, to)?),
        None => None,
    };
    let list = parts.story_specs()?;
    let mut sources = Vec::new();
    let mut items = Vec::new();
    let (mut found, mut blocks) = (0u32, 0u32);
    for spec in &list.0 {
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        if limit.as_ref().is_some_and(|l| l.part != part) {
            continue;
        }
        let source = Source::load(parts, part, mode, true)?;
        let tree = parts.tree(part)?;
        let (lo, hi) = match &limit {
            Some(l) => l.tops(&source)?,
            None => (0, source.tops.len()),
        };
        let mut hits: Vec<(usize, u32)> = Vec::new();
        for (t, top) in source.tops.iter().enumerate().take(hi).skip(lo) {
            let mut matches = 0u32;
            for i in top.first..top.end {
                if source.story.blocks().get(i).map(|e| e.kind) != Some(BlockKind::Paragraph) {
                    continue;
                }
                if let Some(p) = source.story.paragraph(tree, i) {
                    let text: String = p
                        .text(mode)
                        .chars()
                        .filter(|c| !matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
                        .collect();
                    matches = matches.saturating_add(count(&text, wanted, fold));
                }
            }
            if matches > 0 {
                hits.push((t, matches));
                found = found.saturating_add(matches);
                blocks = blocks.saturating_add(1);
            }
        }
        if hits.is_empty() {
            continue;
        }
        let src = sources.len();
        let reach = usize::from(context);
        let mut next = lo;
        for (t, _) in &hits {
            let from = t.saturating_sub(reach).max(next);
            let to = (t + reach + 1).min(hi);
            for top in from..to {
                let m = hits.iter().find(|(h, _)| *h == top).map_or(0, |(_, m)| *m);
                items.push(Item {
                    src,
                    top,
                    matches: m,
                });
            }
            next = to.max(next);
        }
        sources.push(source);
    }
    let place = match within {
        Some((from, Some(to))) => format!(" in {from}..{to}"),
        Some((from, None)) => format!(" in {from}.."),
        None => String::new(),
    };
    let counted = match (found, blocks) {
        (0, _) => "no matches".to_string(),
        (1, _) => "1 match in 1 block".to_string(),
        (n, 1) => format!("{n} matches in 1 block"),
        (n, b) => format!("{n} matches in {b} blocks"),
    };
    Ok(Selected {
        name: "find",
        label: Some(format!("find {needle:?}{place}: {counted}")),
        sources,
        items,
        focus: None,
    })
}

/// The range a find is limited to: a story, and the entries of its first and last block.
struct FindLimit {
    part: PartId,
    first: usize,
    last: Option<usize>,
    from: String,
}

impl FindLimit {
    /// The tops of `source` the range covers, as a start and an end that is not included.
    fn tops(&self, source: &Source) -> Result<(usize, usize), StetError> {
        let lo = source.top_of(self.first).ok_or_else(|| {
            error(
                ErrorCode::NotFound,
                format!("{} is not a block of its story.", self.from),
                "View the outline.",
            )
        })?;
        let hi = match self.last {
            Some(z) => source.top_of(z).ok_or_else(|| {
                error(
                    ErrorCode::NotFound,
                    "the end of the range is not a block.",
                    "View the outline.",
                )
            })?,
            None => source.tops.len().saturating_sub(1),
        };
        if hi < lo {
            return Err(error(
                ErrorCode::InvalidPayload,
                format!(
                    "the end of the range comes before {}, and a range goes forward.",
                    self.from
                ),
                "Swap the two refs.",
            ));
        }
        Ok((lo, hi + 1))
    }
}

/// Resolves the range of a find, which is in one story like any range.
fn find_limit(
    parts: &PartsView<'_>,
    state: &State,
    from: &str,
    to: Option<&str>,
) -> Result<FindLimit, StetError> {
    let (part, first) = locate(parts, state, from)?;
    let last = match to {
        Some(t) => {
            let (other, z) = locate(parts, state, t)?;
            if other != part {
                return Err(error(
                    ErrorCode::InvalidPayload,
                    format!("{from} and {t} are in different stories, and a range is in one."),
                    "Find in each story with a range of its own.",
                ));
            }
            Some(z)
        }
        None => None,
    };
    Ok(FindLimit {
        part,
        first,
        last,
        from: from.to_string(),
    })
}
