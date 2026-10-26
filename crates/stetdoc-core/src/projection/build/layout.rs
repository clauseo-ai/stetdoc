//! The top-level blocks of a story: what a page is made of and what a selection counts.
//!
//! A top is a paragraph, a table with everything in it, or a run of paragraphs that one field covers
//! (a table of contents), which the model shows as one collapsed block. Tops are in entry order, and each
//! belongs to one unit (a note, a comment, or the story itself).

use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::state::{Dep, PartId, PartsView};
use crate::wordml::{BlockKind, FieldInfo, ItemKind, Paragraph, SpanTree, Story};
use std::sync::Arc;

/// A paragraph, a table, or a collapsed field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Top {
    /// The entry where the top starts.
    pub first: usize,
    /// The entry after the last one of the top.
    pub end: usize,
    /// The index of the unit it belongs to, or `NO_UNIT`.
    pub unit: u32,
    /// Set when one field covers this run of paragraphs.
    pub collapse: Option<Collapse>,
}

/// A field that covers many paragraphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Collapse {
    /// The field's keyword, in lower case.
    pub keyword: Box<str>,
    /// The entry of the last paragraph it covers.
    pub last: usize,
    /// How many top-level blocks it covers.
    pub blocks: u32,
}

/// A field must cover at least this many top-level blocks to be collapsed.
const COLLAPSE_AT: usize = 3;

impl PartsView<'_> {
    /// The tops of the story at `part` in `mode`, cached until the part changes. With `expanded` no field is
    /// collapsed, so a selection inside a table of contents shows its paragraphs. Whether a field is collapsed depends on the
    /// mode, as what it would hide does: text that a pending deletion takes out is not in `Final`, so there a field it was
    /// the only text beside is collapsed, as it is once the deletion is accepted.
    pub(crate) fn tops(
        &self,
        part: PartId,
        mode: Mode,
        expanded: bool,
    ) -> Result<Arc<Vec<Top>>, StetError> {
        let build = |_: Option<&Vec<Top>>| -> Result<Vec<Top>, StetError> {
            let story = self.story(part)?;
            let tree = self.tree(part)?;
            Ok(tops_of(&story, tree, (!expanded).then_some(mode)))
        };
        let mode_code = match mode {
            Mode::Final => 0,
            Mode::Original => 1,
            Mode::Redline => 2,
        };
        self.derived(
            (u64::from(part.number()) * 4 + mode_code) * 2 + u64::from(expanded),
            &[Dep::Part(part)],
            &build,
        )
    }
}

/// The tops of a story, in order, with its long fields collapsed as the view of `collapse`'s mode shows them, if it is given.
pub(super) fn tops_of(story: &Story, tree: &SpanTree, collapse: Option<Mode>) -> Vec<Top> {
    let entries = story.blocks();
    let mut tops: Vec<Top> = Vec::new();
    let mut i = 0usize;
    while let Some(entry) = entries.get(i) {
        if entry.parent.is_some() {
            // An entry whose parent was not met: a malformed scan. It is not lost, it is its own top.
            tops.push(Top {
                first: i,
                end: i + 1,
                unit: entry.unit,
                collapse: None,
            });
            i += 1;
            continue;
        }
        let mut end = i + 1;
        if entry.kind == BlockKind::Table {
            while entries
                .get(end)
                .is_some_and(|e| e.parent.is_some_and(|p| p as usize >= i))
            {
                end += 1;
            }
        }
        tops.push(Top {
            first: i,
            end,
            unit: entry.unit,
            collapse: None,
        });
        i = end;
    }
    match collapse {
        Some(mode) => collapse_fields(story, tree, tops, mode),
        None => tops,
    }
}

/// Replaces each run of tops that one field covers by one collapsed top.
fn collapse_fields(story: &Story, tree: &SpanTree, tops: Vec<Top>, mode: Mode) -> Vec<Top> {
    let fields = story.fields(tree);
    let candidates: Vec<(usize, usize, &FieldInfo)> = fields
        .iter()
        .filter(|f| f.parent.is_none())
        .filter_map(|f| {
            let b = story.index_of(f.paragraph_begin)?;
            let e = story.index_of(f.paragraph_end?)?;
            let tb = tops.binary_search_by_key(&b, |t| t.first).ok()?;
            let te = tops.binary_search_by_key(&e, |t| t.first).ok()?;
            (te >= tb + COLLAPSE_AT - 1).then_some((tb, te, f))
        })
        .collect();
    if candidates.is_empty() {
        return tops;
    }
    let mut out: Vec<Top> = Vec::with_capacity(tops.len());
    let mut at = 0usize;
    for (tb, te, f) in candidates {
        if tb < at {
            continue;
        }
        let (Some(first), Some(last)) = (tops.get(tb), tops.get(te)) else {
            continue;
        };
        if first.unit != last.unit
            || !field_is_alone(story, tree, f, (first.first, last.first), mode)
        {
            continue;
        }
        let (Some(tb), Some(te)) = (
            joined_start(story, tree, &tops, (tb, at), mode),
            joined_end(story, tree, &tops, (tb, te), mode),
        ) else {
            continue;
        };
        let (Some(first), Some(last)) = (tops.get(tb), tops.get(te)) else {
            continue;
        };
        out.extend(tops.get(at..tb).unwrap_or_default().iter().cloned());
        out.push(Top {
            first: first.first,
            end: last.end,
            unit: first.unit,
            collapse: Some(Collapse {
                keyword: f.keyword.to_lowercase().into_boxed_str(),
                last: last.first,
                blocks: u32::try_from(te - tb + 1).unwrap_or(u32::MAX),
            }),
        });
        at = te + 1;
    }
    out.extend(tops.get(at..).unwrap_or_default().iter().cloned());
    out
}

/// True if the paragraph where the field begins has no text before it and the paragraph where it ends has
/// none after it, in `mode`, so that collapsing the paragraphs hides nothing else that the view shows.
fn field_is_alone(
    story: &Story,
    tree: &SpanTree,
    f: &FieldInfo,
    (begin, end): (usize, usize),
    mode: Mode,
) -> bool {
    let shown = |i: &crate::wordml::Item| i.len > 0 && i.vis.in_mode(mode);
    let text_before_begin = story
        .paragraph(tree, begin)
        .map(|p: Paragraph<'_>| {
            let map = p.map();
            let at = map.items().iter().position(|i| i.node == f.begin);
            at.is_some_and(|at| map.items().iter().take(at).any(shown))
        })
        .unwrap_or(true);
    let text_after_end = match f.end {
        Some(end_node) => story
            .paragraph(tree, end)
            .map(|p: Paragraph<'_>| {
                let map = p.map();
                let at = map
                    .items()
                    .iter()
                    .rposition(|i| i.node == end_node && i.kind == ItemKind::FieldEnd);
                at.is_some_and(|at| map.items().iter().skip(at + 1).any(shown))
            })
            .unwrap_or(true),
        None => true,
    };
    !text_before_begin && !text_after_end
}

/// True if paragraph `i` shows text in `mode`: an item with width that the mode shows. A field with an empty result is
/// no text, as `field_is_alone` counts it, so the paragraphs a field takes in are judged as the paragraph an accept makes.
fn shows_text(story: &Story, tree: &SpanTree, i: usize, mode: Mode) -> bool {
    story.paragraph(tree, i).is_none_or(|p| {
        p.map()
            .items()
            .iter()
            .any(|item| item.len > 0 && item.vis.in_mode(mode))
    })
}

/// The index of the first top a collapsed field covers, from the top where it begins `tb` in `mode`, or `None` when the
/// field cannot collapse. A paragraph before it whose mark is gone in `mode` joins the paragraph where the field begins: one
/// that shows text keeps the field open, and one that shows none is in the field's tops, as it is in the paragraph an accept
/// leaves. A top that an earlier field took (before `at`) is not taken again.
fn joined_start(
    story: &Story,
    tree: &SpanTree,
    tops: &[Top],
    (tb, at): (usize, usize),
    mode: Mode,
) -> Option<usize> {
    let mut first = tb;
    while let Some(prev) = first.checked_sub(1) {
        let (top, begin) = (tops.get(prev)?, tops.get(first)?.first);
        if top.collapse.is_some()
            || top.end != begin
            || story.join_run(tree, top.first, mode).end <= begin
        {
            return Some(first);
        }
        if prev < at || top.unit != tops.get(tb)?.unit || shows_text(story, tree, top.first, mode) {
            return None;
        }
        first = prev;
    }
    Some(first)
}

/// The index of the last top a collapsed field covers, from its first and last tops `(tb, te)` in `mode`, or `None` when the
/// field cannot collapse. The paragraph where the field ends ends the field's tops unless its mark is gone in `mode`: then it
/// joins the paragraphs after it, which must show nothing, and the last of them is in the field's tops, as it is in the
/// paragraph an accept leaves. In `Redline` a revision on that mark is shown, so the field stays open.
fn joined_end(
    story: &Story,
    tree: &SpanTree,
    tops: &[Top],
    (tb, te): (usize, usize),
    mode: Mode,
) -> Option<usize> {
    let end = tops.get(te)?.first;
    let p = story.paragraph(tree, end)?;
    if mode == Mode::Redline && !p.mark_revs().is_none() {
        return None;
    }
    let run = story.join_run(tree, end, mode);
    if run.end <= end + 1 {
        return Some(te);
    }
    let joined_show_nothing = (end + 1..run.end).all(|k| !shows_text(story, tree, k, mode));
    let last = tops
        .binary_search_by_key(&(run.end - 1), |t| t.first)
        .ok()?;
    let same_unit = tops.get(tb)?.unit == tops.get(last)?.unit;
    (joined_show_nothing && same_unit).then_some(last)
}
