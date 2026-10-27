//! `occurrences`: every place a text is in the paragraphs of a scope, matched as an operation's `find` matches it, so that
//! a caller can turn each place into an operation against the same state.
//!
//! A paragraph's matches are those of `find` in its Final text: exact, or, when nothing matches exactly, with curly and
//! straight quotes taken as the same (`ops::target::find` does the same two tries). A paragraph where the text is only in
//! a tracked deletion is listed apart, because an operation cannot target it.

use super::place::{Placer, heading_text};
use super::select::{Selected, clauses_scope, range_scope, story_scope};
use crate::api::error::{ErrorCode, StetError};
use crate::api::occurrence::{Occurrence, Occurrences};
use crate::api::view::{Mode, Scope, StoryName};
use crate::state::{PartsView, State};
use crate::wordml::{BlockKind, StoryKind, fold_quotes};
use std::ops::Range;

/// How many characters a snippet shows on each side of a match.
const SIDE: usize = 30;

/// Every place `needle` is in the paragraphs `scope` selects. `Scope::Auto` is the body, the notes and the headers and
/// footers; `Range`, `Clause` and `Story` select as a view does; any other scope is `invalid_payload`.
/// With `ignore_case`, letters match whatever their case, one character for one, so the offsets stay those of the Final text.
pub(crate) fn occurrences(
    state: &State,
    needle: &str,
    scope: &Scope,
    ignore_case: bool,
) -> Result<Occurrences, StetError> {
    if needle.is_empty() {
        return Err(
            StetError::new(ErrorCode::InvalidPayload, "the text to find is empty.")
                .with_next("Give the text as the document reads."),
        );
    }
    let parts = state.parts();
    let selections = match scope {
        Scope::Auto => [StoryName::Body, StoryName::Notes, StoryName::Headers]
            .into_iter()
            .map(|name| story_scope(&parts, Mode::Final, name))
            .collect::<Result<Vec<_>, _>>()?,
        Scope::Story(name) => vec![story_scope(&parts, Mode::Final, *name)?],
        Scope::Range { from, to } => vec![range_scope(
            &parts,
            state,
            Mode::Final,
            from,
            to.as_deref(),
        )?],
        Scope::Clause(texts) => vec![clauses_scope(&parts, Mode::Final, texts)?],
        _ => {
            return Err(StetError::new(
                ErrorCode::InvalidPayload,
                "occurrences are found in the whole document, a story, a range or a clause.",
            )
            .with_next("Pass no scope, a range of refs, or a clause number."));
        }
    };
    let index = parts.doc_index()?;
    let mut placer = Placer::new(parts, &index)?;
    let mut hits = Vec::new();
    let mut deleted = Vec::new();
    for selected in &selections {
        scan(
            &parts,
            &mut placer,
            selected,
            needle,
            ignore_case,
            &mut hits,
            &mut deleted,
        )?;
    }
    Ok(Occurrences {
        base: state.base_token().to_string(),
        needle: needle.to_string(),
        hits,
        deleted,
    })
}

/// Adds the matches in the paragraphs of one selection.
fn scan(
    parts: &PartsView<'_>,
    placer: &mut Placer<'_, '_>,
    selected: &Selected,
    needle: &str,
    ignore_case: bool,
    hits: &mut Vec<Occurrence>,
    deleted: &mut Vec<Occurrence>,
) -> Result<(), StetError> {
    let folded_needle = fold_quotes(needle);
    let lowered_needle = ignore_case.then(|| fold_case(needle));
    for item in &selected.items {
        let Some(source) = selected.sources.get(item.src) else {
            continue;
        };
        let Some(top) = source.tops.get(item.top) else {
            continue;
        };
        let tree = parts.tree(source.part)?;
        let story = story_word(parts.story_kind(source.part).ok());
        for entry in top.first..top.end {
            let Some(block) = source.story.blocks().get(entry) else {
                continue;
            };
            if block.kind != BlockKind::Paragraph {
                continue;
            }
            let Some(paragraph) = source.story.paragraph(tree, entry) else {
                continue;
            };
            let key = source.story.key(block);
            let Some(r) = parts.ref_of(&key) else {
                continue;
            };
            let map = paragraph.map();
            let text = map.text(Mode::Final).into_owned();
            let mut loose = false;
            let mut found = match &lowered_needle {
                Some(lowered) => ranges(&fold_case(&text), lowered),
                None => map.find_all(Mode::Final, needle),
            };
            if found.is_empty() {
                found = match &lowered_needle {
                    Some(lowered) => ranges(&fold_quotes(&fold_case(&text)), &fold_quotes(lowered)),
                    None => ranges(&fold_quotes(&text), &folded_needle),
                };
                loose = true;
            }
            let place = placer.place(source.part, &source.story, entry)?;
            let clause = place.clause.map(|c| c.full.clone());
            let heading = match place.heading {
                Some(h) => Some(heading_text(parts, source.part, &source.story, h)?),
                None => None,
            };
            let place = |n: usize, range: &Range<usize>, text: &str, loose: bool| Occurrence {
                block: r.to_string(),
                story: story.to_string(),
                clause: clause.clone(),
                heading: heading.clone(),
                occurrence: u32::try_from(n + 1).unwrap_or(u32::MAX),
                start: u32::try_from(range.start).unwrap_or(u32::MAX),
                end: u32::try_from(range.end).unwrap_or(u32::MAX),
                snippet: snippet(text, range),
                loose,
            };
            if found.is_empty() {
                let original = match &lowered_needle {
                    Some(lowered) => ranges(&fold_case(&map.text(Mode::Original)), lowered),
                    None => map.find_all(Mode::Original, needle),
                };
                if let Some(range) = original.first() {
                    let text = map.text(Mode::Original);
                    deleted.push(place(0, range, &text, false));
                }
                continue;
            }
            hits.extend(
                found
                    .iter()
                    .enumerate()
                    .map(|(n, range)| place(n, range, &text, loose)),
            );
        }
    }
    Ok(())
}

/// The name of a story kind, as an `Occurrence` carries it.
fn story_word(kind: Option<StoryKind>) -> &'static str {
    match kind {
        Some(StoryKind::Header | StoryKind::Footer) => "headers",
        Some(StoryKind::Footnotes | StoryKind::Endnotes) => "notes",
        Some(StoryKind::Comments) => "comments",
        Some(StoryKind::Body) | None => "body",
    }
}

/// The text with each letter in lower case, one character for one: a letter whose lower case is more than one character
/// stays as it is, so that a character offset in the result is the same offset in `text`.
fn fold_case(text: &str) -> String {
    text.chars()
        .map(|c| {
            let mut low = c.to_lowercase();
            match (low.next(), low.next()) {
                (Some(one), None) => one,
                _ => c,
            }
        })
        .collect()
}

/// The occurrences of `needle` in `text`, not overlapping, as character ranges.
fn ranges(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let width = needle.chars().count();
    let mut out = Vec::new();
    let (mut byte, mut chars) = (0usize, 0usize);
    for (at, _) in text.match_indices(needle) {
        chars += text.get(byte..at).map_or(0, |s| s.chars().count());
        byte = at;
        out.push(chars..chars + width);
    }
    out
}

/// The match with up to `SIDE` characters on each side, the match in square brackets, on one line, without the
/// placeholders of notes and objects.
fn snippet(text: &str, range: &Range<usize>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let from = range.start.saturating_sub(SIDE);
    let to = (range.end + SIDE).min(chars.len());
    let part = |a: usize, b: usize| -> String {
        chars
            .get(a..b)
            .map(|s| {
                s.iter()
                    .filter(|c| !matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
                    .map(|c| if c.is_whitespace() { ' ' } else { *c })
                    .collect::<String>()
            })
            .unwrap_or_default()
    };
    format!(
        "{}{}[{}]{}{}",
        if from > 0 { "…" } else { "" },
        part(from, range.start),
        part(range.start, range.end),
        part(range.end, to),
        if to < chars.len() { "…" } else { "" },
    )
}
