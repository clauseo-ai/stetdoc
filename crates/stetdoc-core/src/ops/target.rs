//! How an operation names what it works on: refs, `find`, offsets and ranges (design `document-and-ops.md` 2.2).
//!
//! A ref is resolved against the base token the batch names. A range inside a paragraph is `find` (which must match once, or
//! `occurrence` picks one), or `start` and `end`, or nothing for the whole paragraph, and it is read from the paragraph's Final
//! text as the operation finds it, so an operation sees what the one before it left.

use super::quotes;
use crate::api::error::{ErrorCode, StetError};
use crate::api::options::OffsetUnit;
use crate::api::view::{Mode, Warning, WarningCode};
use crate::refs::{BaseToken, Claim};
use crate::state::Tx;
use crate::wordml::{BlockKey, BlockKind, StoryKind, TextMap, WrapperKind, revision_at};
use std::ops::Range;

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The words a warning uses for a story.
pub(super) fn story_label(kind: StoryKind) -> &'static str {
    match kind {
        StoryKind::Body => "body",
        StoryKind::Header | StoryKind::Footer => "headers",
        StoryKind::Footnotes | StoryKind::Endnotes => "notes",
        StoryKind::Comments => "comments",
    }
}

/// A warning about a block, or about an operation when `block` is `None`.
pub(super) fn warning(
    code: WarningCode,
    tx: &Tx<'_>,
    block: Option<&BlockKey>,
    message: impl Into<String>,
) -> Warning {
    let (shown, story) = match block {
        Some(key) => (
            tx.parts().ref_of(key).map(|r| r.to_string()),
            story_of(tx, key).map(|kind| story_label(kind).to_string()),
        ),
        None => (None, None),
    };
    Warning {
        code,
        block: shown,
        story,
        message: message.into(),
        next: None,
    }
}

/// The kind of story a block is in.
pub(super) fn story_of(tx: &Tx<'_>, key: &BlockKey) -> Option<StoryKind> {
    let view = tx.parts();
    let part = view.id(key.part())?;
    view.story_kind(part).ok()
}

/// The block a ref names, as the transaction sees it: the claim is the batch's base token.
pub(super) fn block(tx: &Tx<'_>, base: &BaseToken, text: &str) -> Result<BlockKey, StetError> {
    tx.resolve(Claim::Base(base), text)
}

/// What kind of block a key is, and its index among the blocks of its story.
pub(super) fn kind_of(tx: &Tx<'_>, key: &BlockKey) -> Result<(BlockKind, usize), StetError> {
    let view = tx.parts();
    let missing = || {
        StetError::new(
            ErrorCode::StaleRef,
            "that block is not in the document any more",
        )
    };
    let part = view.id(key.part()).ok_or_else(missing)?;
    let story = view.story(part)?;
    let index = story.index_of(key.node()).ok_or_else(missing)?;
    let entry = story.blocks().get(index).ok_or_else(missing)?;
    Ok((entry.kind, index))
}

/// A paragraph read for an operation.
pub(super) struct Para {
    /// The paragraph.
    pub key: BlockKey,
    /// The text model of the paragraph.
    pub map: TextMap,
    /// Its Final text.
    pub text: String,
    /// How many characters the Final text has.
    pub len: usize,
}

/// Reads the paragraph `text` names. A table or a row is `cross_block`, and the message says which.
pub(super) fn paragraph(tx: &Tx<'_>, key: &BlockKey, text: &str) -> Result<Para, StetError> {
    let (kind, index) = kind_of(tx, key)?;
    match kind {
        BlockKind::Paragraph => {}
        BlockKind::Table => {
            return Err(StetError::new(
                ErrorCode::CrossBlock,
                format!("{text} is a table. Name a paragraph in one of its cells."),
            ));
        }
        BlockKind::Row => {
            return Err(StetError::new(
                ErrorCode::CrossBlock,
                format!("{text} is a table row. Name a paragraph in one of its cells."),
            ));
        }
    }
    read(tx, key, index)
}

/// Reads the paragraph at an entry of its story.
fn read(tx: &Tx<'_>, key: &BlockKey, index: usize) -> Result<Para, StetError> {
    let view = tx.parts();
    let internal = |m: &str| StetError::new(ErrorCode::Internal, format!("internal: {m}"));
    let part = view
        .id(key.part())
        .ok_or_else(|| internal("a block's part is not in the document"))?;
    let story = view.story(part)?;
    let tree = view.tree(part)?;
    let paragraph = story
        .paragraph(tree, index)
        .ok_or_else(|| internal("a paragraph entry has no paragraph"))?;
    let map = paragraph.map();
    let text = map.text(Mode::Final).into_owned();
    let len = text.chars().count();
    Ok(Para {
        key: key.clone(),
        map,
        text,
        len,
    })
}

/// The Final text of the paragraphs next to a block, nearest first, from the same container: the context of the quote policy.
/// Up to five on each side, alternating before and after.
pub(super) fn neighbour_texts(tx: &Tx<'_>, key: &BlockKey) -> Vec<String> {
    let view = tx.parts();
    let found = (|| {
        let part = view.id(key.part())?;
        let story = view.story(part).ok()?;
        let tree = view.tree(part).ok()?;
        let index = story.index_of(key.node())?;
        let parent = story.blocks().get(index)?.parent;
        let text_of = |i: usize| -> Option<String> {
            let entry = story.blocks().get(i)?;
            if entry.parent != parent || entry.kind != BlockKind::Paragraph {
                return None;
            }
            Some(
                story
                    .paragraph(tree, i)?
                    .map()
                    .text(Mode::Final)
                    .into_owned(),
            )
        };
        let mut out = Vec::new();
        for distance in 1..=5usize {
            if let Some(i) = index.checked_sub(distance)
                && let Some(t) = text_of(i)
            {
                out.push(t);
            }
            if let Some(t) = text_of(index + distance) {
                out.push(t);
            }
        }
        Some(out)
    })();
    found.unwrap_or_default()
}

/// The quote context for text that goes into or next to `key`: the paragraph's own text, then its neighbours.
pub(super) fn quote_context(tx: &Tx<'_>, key: &BlockKey) -> quotes::Context {
    let own = match kind_of(tx, key) {
        Ok((BlockKind::Paragraph, index)) => {
            read(tx, key, index).map(|p| p.text).unwrap_or_default()
        }
        _ => String::new(),
    };
    let neighbours = neighbour_texts(tx, key);
    quotes::context(&own, neighbours.iter().map(String::as_str))
}

/// What names a range inside a paragraph, as the payload gave it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Within<'a> {
    pub find: Option<&'a str>,
    pub occurrence: Option<u32>,
    pub start: Option<u32>,
    pub end: Option<u32>,
}

impl Within<'_> {
    /// True when the payload names no range: the whole paragraph.
    pub(super) fn is_whole(&self) -> bool {
        self.find.is_none() && self.start.is_none() && self.end.is_none()
    }
}

/// A range found, and whether only the quote-folded second try found it.
pub(super) struct Found {
    pub range: Range<usize>,
    pub loose: bool,
}

/// The occurrences of `needle` in `text`, not overlapping, as character ranges.
fn occurrences(text: &str, needle: &str) -> Vec<Range<usize>> {
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

/// A few characters either side of a match, for a message.
fn context_of(text: &str, range: &Range<usize>) -> String {
    let chars: Vec<char> = text.chars().collect();
    let from = range.start.saturating_sub(20);
    let to = (range.end + 20).min(chars.len());
    let part = |a: usize, b: usize| -> String {
        chars
            .get(a..b)
            .map(|s| s.iter().collect::<String>())
            .unwrap_or_default()
            .replace('\n', " ")
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

/// Which of several matches, or the one: `ambiguous_match` says where they are.
fn pick(
    name: &str,
    find: &str,
    text: &str,
    matches: Vec<Range<usize>>,
    occurrence: Option<u32>,
    loose: bool,
) -> Result<Found, StetError> {
    let count = matches.len();
    match occurrence {
        Some(n) => {
            let at = (n as usize).checked_sub(1).and_then(|i| matches.get(i));
            match at {
                Some(range) => Ok(Found {
                    range: range.clone(),
                    loose,
                }),
                None => Err(StetError::new(
                    ErrorCode::NotFound,
                    format!("`{find}` matches {count} time{} in {name}, and there is no occurrence {n}.", if count == 1 { "" } else { "s" }),
                )
                .with_next("Count the matches from 1, or add words to `find` so that it matches once.")),
            }
        }
        None if count == 1 => Ok(Found {
            range: matches.into_iter().next().unwrap_or(0..0),
            loose,
        }),
        None => {
            let shown: Vec<String> = matches
                .iter()
                .take(5)
                .enumerate()
                .map(|(i, r)| format!("{}: at {} {}", i + 1, r.start, context_of(text, r)))
                .collect();
            Err(StetError::new(
                ErrorCode::AmbiguousMatch,
                format!(
                    "`{find}` matches {count} times in {name}{}: {}.",
                    if count > 5 { " (the first five are shown)" } else { "" },
                    shown.join("; ")
                ),
            )
            .with_next("Say which with `occurrence` (counted from 1), or add words to `find` so that it matches once."))
        }
    }
}

/// Where `find` is in the paragraph's Final text.
pub(super) fn find(
    para: &Para,
    name: &str,
    needle: &str,
    occurrence: Option<u32>,
    tx: &Tx<'_>,
) -> Result<Found, StetError> {
    if needle.is_empty() {
        return Err(invalid("`find` is empty."));
    }
    let exact = para.map.find_all(Mode::Final, needle);
    if !exact.is_empty() {
        return pick(name, needle, &para.text, exact, occurrence, false);
    }
    // The one place where `find` is not exact: curly and straight quotes are the same, for agents that type straight quotes
    // into documents that use curly ones.
    let folded_text = quotes::fold(&para.text);
    let folded = occurrences(&folded_text, &quotes::fold(needle));
    if !folded.is_empty() {
        return pick(name, needle, &para.text, folded, occurrence, true);
    }
    // Text that is only in a deletion is said to be.
    let original = para.map.find_all(Mode::Original, needle);
    if let Some(range) = original.first() {
        let whose = deleted_by(tx, para, range);
        return Err(StetError::new(
            ErrorCode::NotFound,
            format!(
                "`{needle}` is in {name} only as text that a tracked deletion{whose} removes, so it is not in the paragraph as it reads now."
            ),
        )
        .with_next("Reject that change, or edit another paragraph."));
    }
    let preview: String = para.text.chars().take(60).collect();
    Err(StetError::new(
        ErrorCode::NotFound,
        format!(
            "`{needle}` is not in {name}, which reads: {preview:?}{}",
            if para.len > 60 { "…" } else { "" }
        ),
    )
    .with_next(
        "View the paragraph for its text, and write `find` as it reads, or use `start` and `end`.",
    ))
}

/// Whose deletion a range of the Original text is in, as ` by JD`, or nothing if it cannot be told.
fn deleted_by(tx: &Tx<'_>, para: &Para, range: &Range<usize>) -> String {
    let view = tx.parts();
    let author = (|| {
        let part = view.id(para.key.part())?;
        let tree = view.tree(part).ok()?;
        let mut at = 0usize;
        for item in para.map.items() {
            let end = at + item.len as usize;
            let in_original = item.vis.in_mode(Mode::Original);
            if in_original && item.len > 0 {
                if end > range.start && at < range.end {
                    for context in para.map.context(item.ctx) {
                        if context.kind == WrapperKind::Del {
                            return revision_at(&tree.node_ref(context.node))
                                .and_then(|r| r.author);
                        }
                    }
                }
                at = end;
            }
        }
        None
    })();
    author.map(|a| format!(" by {a}")).unwrap_or_default()
}

/// One offset, in the unit the edit counts, as a scalar offset in the paragraph's Final text.
pub(super) fn offset(
    para: &Para,
    name: &str,
    value: u32,
    unit: OffsetUnit,
) -> Result<usize, StetError> {
    let wanted = value as usize;
    let at = match unit {
        OffsetUnit::Scalar => (wanted <= para.len).then_some(wanted),
        OffsetUnit::Utf16 => para.map.utf16_to_char(Mode::Final, wanted),
    };
    at.ok_or_else(|| {
        invalid(match unit {
            OffsetUnit::Scalar => format!("the offset {value} is past the end of {name}, which has {} characters.", para.len),
            OffsetUnit::Utf16 => format!(
                "the offset {value} is past the end of {name} or inside a character that takes two UTF-16 units or is an atom."
            ),
        })
    })
}

/// The range of a paragraph that a payload names: `find`, `start` and `end`, or the whole paragraph.
pub(super) fn range(
    tx: &Tx<'_>,
    para: &Para,
    name: &str,
    within: &Within<'_>,
    unit: OffsetUnit,
) -> Result<Found, StetError> {
    if let Some(needle) = within.find {
        return find(para, name, needle, within.occurrence, tx);
    }
    match (within.start, within.end) {
        (None, None) => Ok(Found {
            range: 0..para.len,
            loose: false,
        }),
        (Some(start), Some(end)) => {
            let (start, end) = (
                offset(para, name, start, unit)?,
                offset(para, name, end, unit)?,
            );
            if start > end {
                return Err(invalid(format!(
                    "`start` ({start}) is after `end` ({end})."
                )));
            }
            Ok(Found {
                range: start..end,
                loose: false,
            })
        }
        _ => Err(invalid("`start` and `end` go together.")),
    }
}
