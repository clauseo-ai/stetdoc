//! The `Find` scope: the blocks whose text holds a needle, with their neighbours, in every story or within one range.

use super::needles::{Needles, Snippet, snippet};
use super::select::{Item, Selected, Source, error, locate};
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::Mode;
use crate::state::{PartId, PartsView, State};
use crate::wordml::BlockKind;

/// About how many characters of a paragraph a compact hit shows around the match.
pub(crate) const SNIPPET: usize = 100;

/// The longest paragraph, in characters, that may be a heading, for the tips of a compact page.
const HEADING: usize = 80;

/// What a find found beyond the items: each paragraph that matched, for the compact page and for the margins that say
/// which needles a block holds.
pub(crate) struct Found {
    /// The needles as given, each once.
    pub needles: Vec<String>,
    /// The paragraphs that hold a needle, in the order of the items.
    pub hits: Vec<FoundHit>,
}

/// One paragraph that holds a needle.
pub(crate) struct FoundHit {
    /// The item of the top it is in.
    pub item: usize,
    /// Its entry in the story of that item's source.
    pub entry: usize,
    /// How many times it holds each needle, in the order of the needles.
    pub counts: Vec<u32>,
    /// The text around the first match.
    pub snippet: Snippet,
    /// The needles are only in deleted text, and the snippet is the start of the paragraph.
    pub deleted_only: bool,
    /// It is short and does not end like prose, so it may be a heading.
    pub short: bool,
}

impl Found {
    /// How many times the item holds each needle, in the order of the needles.
    pub(crate) fn counts_of(&self, item: usize) -> Vec<u32> {
        let mut sum = vec![0u32; self.needles.len()];
        for hit in self.hits.iter().filter(|h| h.item == item) {
            for (s, c) in sum.iter_mut().zip(&hit.counts) {
                *s = s.saturating_add(*c);
            }
        }
        sum
    }
}

/// The tops that hold any of the needles, with `context` tops on each side, in every story, or only in the range
/// `within`: from a ref to a ref, or to the end of the story of the first. The label counts the matches of each needle
/// and the blocks that hold them.
pub(crate) fn find_scope(
    parts: &PartsView<'_>,
    state: &State,
    mode: Mode,
    given: &[String],
    context: u8,
    within: Option<(&str, Option<&str>)>,
) -> Result<(Selected, Found), StetError> {
    if given.is_empty() {
        return Err(error(
            ErrorCode::InvalidPayload,
            "a find needs some text to look for.",
            "Give the text to find.",
        ));
    }
    let needles = Needles::parse(given, "a find")?;
    let limit = match within {
        Some((from, to)) => Some(find_limit(parts, state, from, to)?),
        None => None,
    };
    let list = parts.story_specs()?;
    let mut sources = Vec::new();
    let mut items = Vec::new();
    let mut hits = Vec::new();
    let mut per_needle = vec![0u32; needles.len()];
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
        let mut tops: Vec<(usize, u32, Vec<Para>)> = Vec::new();
        for (t, top) in source.tops.iter().enumerate().take(hi).skip(lo) {
            let mut matches = 0u32;
            let mut paras = Vec::new();
            for i in top.first..top.end {
                if source.story.blocks().get(i).map(|e| e.kind) != Some(BlockKind::Paragraph) {
                    continue;
                }
                let Some(p) = source.story.paragraph(tree, i) else {
                    continue;
                };
                let text = clean(&p.text(mode));
                let counts = needles.counts(&text);
                let here = counts.iter().fold(0u32, |a, c| a.saturating_add(*c));
                if here == 0 {
                    continue;
                }
                for (n, c) in per_needle.iter_mut().zip(&counts) {
                    *n = n.saturating_add(*c);
                }
                matches = matches.saturating_add(here);
                // A redline snippet is the text as it will be, which is what an edit finds.
                let shown = if mode == Mode::Redline {
                    clean(&p.text(Mode::Final))
                } else {
                    text
                };
                let at = needles.first_match(&shown);
                let short = shown.chars().count() <= HEADING
                    && !shown.trim_end().ends_with(['.', ';', ',', ':']);
                paras.push(Para {
                    entry: i,
                    counts,
                    snippet: snippet(&shown, at.clone().unwrap_or(0..0), SNIPPET),
                    deleted_only: at.is_none(),
                    short,
                });
            }
            if matches > 0 {
                // A one-line list counts the paragraphs it lists, and whole blocks count the tables as one each.
                let counted = if context == 0 { paras.len() } else { 1 };
                blocks = blocks.saturating_add(u32::try_from(counted).unwrap_or(u32::MAX));
                tops.push((t, matches, paras));
                found = found.saturating_add(matches);
            }
        }
        if tops.is_empty() {
            continue;
        }
        let src = sources.len();
        let reach = usize::from(context);
        let mut next = lo;
        let matched: Vec<usize> = tops.iter().map(|(t, _, _)| *t).collect();
        for t in &matched {
            let from = t.saturating_sub(reach).max(next);
            let to = (t + reach + 1).min(hi);
            for top in from..to {
                let matched = tops.iter_mut().find(|(h, _, _)| *h == top);
                let matches = matched.as_ref().map_or(0, |(_, m, _)| *m);
                if let Some((_, _, paras)) = matched {
                    for para in paras.drain(..) {
                        hits.push(FoundHit {
                            item: items.len(),
                            entry: para.entry,
                            counts: para.counts,
                            snippet: para.snippet,
                            deleted_only: para.deleted_only,
                            short: para.short,
                        });
                    }
                }
                items.push(Item { src, top, matches });
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
    let named: Vec<String> = needles.given().map(|g| format!("{g:?}")).collect();
    let what = if needles.len() == 1 {
        named.concat()
    } else {
        named
            .iter()
            .zip(&per_needle)
            .map(|(g, n)| format!("{g} ({n})"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    Ok((
        Selected {
            name: "find",
            label: Some(format!("find {what}{place}: {counted}")),
            sources,
            items,
            focus: Vec::new(),
        },
        Found {
            needles: needles.given().map(str::to_string).collect(),
            hits,
        },
    ))
}

/// A paragraph that matched, before its item is known.
struct Para {
    entry: usize,
    counts: Vec<u32>,
    snippet: Snippet,
    deleted_only: bool,
    short: bool,
}

/// The text of a paragraph as a find reads it: without the characters that stand for atoms.
fn clean(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
        .collect()
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
