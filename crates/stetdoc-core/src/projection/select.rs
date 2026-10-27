//! Scopes into selections: which blocks of which stories a view is about, as a list of items.
//!
//! An item is one top of one story, which is the unit of a page: a paragraph, a table, or a collapsed
//! field. A selection lists its items in the order of the stories and of the document, and a page is a run
//! of them. A cursor names the last block a page showed, and the next page starts at the item after the one
//! that holds it (design 4.2 and 4.5).

use super::build::Top;
use super::build::scan::{Scan, TopStat};
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::{Mode, StoryName};
use crate::refs::{BlockKey, Claim};
use crate::state::{PartId, PartsView, State};
use crate::wordml::{BlockKind, Story, StoryKind};
use std::cell::OnceCell;
use std::sync::Arc;

/// One story that a selection takes blocks from.
pub(crate) struct Source {
    /// The story's part.
    pub part: PartId,
    /// The story's blocks.
    pub story: Arc<Story>,
    /// The story's tops, with or without the collapsed fields.
    pub tops: Arc<Vec<Top>>,
    /// The mode the sizes of the tops are measured in.
    pub mode: Mode,
    /// How big each top is, once a page has asked.
    sizes: OnceCell<Sizes>,
    /// The tops are those of the layout with no field collapsed.
    pub expanded: bool,
}

/// The sizes of the tops of a source, from where they were made.
enum Sizes {
    Scan(Arc<Scan>),
    Facts(Arc<Vec<TopStat>>),
}

impl Sizes {
    fn as_slice(&self) -> &[TopStat] {
        match self {
            Sizes::Scan(scan) => &scan.tops,
            Sizes::Facts(sizes) => sizes,
        }
    }
}

/// One top of one source, in the order of the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Item {
    /// The source.
    pub src: usize,
    /// The top.
    pub top: usize,
    /// How many times a find's needle is in it. Zero for the other scopes and for context.
    pub matches: u32,
}

/// What a scope selects.
pub(crate) struct Selected {
    /// What the page says it is a page of.
    pub name: &'static str,
    /// A description of the scope for the position line, when it is not the first and last ref.
    pub label: Option<String>,
    /// The stories.
    pub sources: Vec<Source>,
    /// The items, in order.
    pub items: Vec<Item>,
    /// The block the selection is about, when it is about one: the clause of a clause view.
    pub focus: Option<BlockKey>,
}

impl Selected {
    /// Makes the sizes of every source, so that an error in making them is one the caller sees.
    pub(crate) fn load_sizes(
        &self,
        parts: &PartsView<'_>,
        by_facts: bool,
    ) -> Result<(), StetError> {
        self.sources
            .iter()
            .try_for_each(|s| s.sizes(parts, by_facts).map(|_| ()))
    }
}

/// Where a page starts: an item, and for a table, the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Start {
    /// The item.
    pub item: usize,
    /// The first row to show of a table. Zero for everything else.
    pub row: usize,
}

pub(super) fn error(code: ErrorCode, message: impl Into<String>, next: &str) -> StetError {
    StetError::new(code, message).with_next(next)
}

impl Source {
    /// Loads a story for a selection.
    pub(crate) fn load(
        parts: &PartsView<'_>,
        part: PartId,
        mode: Mode,
        expanded: bool,
    ) -> Result<Source, StetError> {
        let story = parts.story(part)?;
        Ok(Source {
            part,
            tops: parts.tops(part, mode, expanded)?,
            story,
            mode,
            sizes: OnceCell::new(),
            expanded,
        })
    }

    /// How big each top is, made when a page first asks and kept, so that the weight of an item is a look-up.
    /// `by_facts` takes the lengths from the index's pass over the text, which is free for a page that has notes
    /// because it needs the index, and takes them from the scan otherwise, which a view that does not need
    /// the index and may become an outline would have to make anyway. Either gives the sizes of law P9.
    pub(crate) fn sizes(
        &self,
        parts: &PartsView<'_>,
        by_facts: bool,
    ) -> Result<&[TopStat], StetError> {
        if let Some(sizes) = self.sizes.get() {
            return Ok(sizes.as_slice());
        }
        let built = if by_facts {
            Sizes::Facts(parts.top_sizes(self.part, self.mode, self.expanded)?)
        } else {
            Sizes::Scan(parts.scan(self.part, self.mode, self.expanded)?)
        };
        Ok(self.sizes.get_or_init(|| built).as_slice())
    }

    /// The top that holds an entry.
    pub(crate) fn top_of(&self, entry: usize) -> Option<usize> {
        let at = self.tops.partition_point(|t| t.end <= entry);
        self.tops.get(at).filter(|t| t.first <= entry).map(|_| at)
    }

    /// The entries of the rows of the table at the top.
    pub(crate) fn rows_of(&self, top: usize) -> Vec<usize> {
        let Some(t) = self.tops.get(top) else {
            return Vec::new();
        };
        if self.story.blocks().get(t.first).map(|e| e.kind) != Some(BlockKind::Table) {
            return Vec::new();
        }
        (t.first + 1..t.end)
            .filter(|i| {
                self.story.blocks().get(*i).is_some_and(|e| {
                    e.kind == BlockKind::Row && e.parent == u32::try_from(t.first).ok()
                })
            })
            .collect()
    }
}

/// The parts of the stories of a group, in the order of the story list.
fn group_parts(parts: &PartsView<'_>, kinds: &[StoryKind]) -> Result<Vec<PartId>, StetError> {
    let list = parts.story_specs()?;
    Ok(list
        .0
        .iter()
        .filter(|s| kinds.contains(&s.kind))
        .filter_map(|s| parts.id(&s.part))
        .collect())
}

/// Every top of the stories of a group.
pub(crate) fn story_scope(
    parts: &PartsView<'_>,
    mode: Mode,
    name: StoryName,
) -> Result<Selected, StetError> {
    let (label, kinds): (&'static str, &[StoryKind]) = match name {
        StoryName::Body => ("body", &[StoryKind::Body]),
        StoryName::Notes => ("notes", &[StoryKind::Footnotes, StoryKind::Endnotes]),
        StoryName::Headers => ("headers", &[StoryKind::Header, StoryKind::Footer]),
        StoryName::Comments => ("comments", &[StoryKind::Comments]),
    };
    let mut sources = Vec::new();
    let mut items = Vec::new();
    for part in group_parts(parts, kinds)? {
        let source = Source::load(parts, part, mode, false)?;
        let src = sources.len();
        items.extend((0..source.tops.len()).map(|top| Item {
            src,
            top,
            matches: 0,
        }));
        sources.push(source);
    }
    Ok(Selected {
        name: label,
        label: None,
        sources,
        items,
        focus: None,
    })
}

/// The part and the entry that a ref names.
pub(crate) fn locate(
    parts: &PartsView<'_>,
    state: &State,
    text: &str,
) -> Result<(PartId, usize), StetError> {
    let key: BlockKey = state.resolve(Claim::Implied, text)?;
    let part = parts.id(key.part()).ok_or_else(|| {
        error(
            ErrorCode::NotFound,
            format!("{text} is not in a part of this document."),
            "Look at the outline for the refs that exist.",
        )
    })?;
    let story = parts.story(part)?;
    let entry = story.index_of(key.node()).ok_or_else(|| {
        error(
            ErrorCode::StaleRef,
            format!("{text} is not in the document any more."),
            "View the document again from the outline.",
        )
    })?;
    Ok((part, entry))
}

/// The tops from one ref to another, or to the end of the story.
pub(crate) fn range_scope(
    parts: &PartsView<'_>,
    state: &State,
    mode: Mode,
    from: &str,
    to: Option<&str>,
) -> Result<Selected, StetError> {
    let (part, a) = locate(parts, state, from)?;
    let end = to.map(|t| locate(parts, state, t)).transpose()?;
    if let Some((other, _)) = end
        && other != part
    {
        return Err(error(
            ErrorCode::InvalidPayload,
            format!(
                "{from} and {} are in different stories, and a range is in one.",
                to.unwrap_or_default()
            ),
            "Use two views, one for each story.",
        ));
    }
    let collapsed = Source::load(parts, part, mode, false)?;
    let inside = |entry: usize| {
        collapsed
            .top_of(entry)
            .and_then(|t| collapsed.tops.get(t))
            .is_some_and(|t| t.collapse.is_some())
    };
    let expanded = inside(a) || end.is_some_and(|(_, z)| inside(z));
    let source = if expanded {
        Source::load(parts, part, mode, true)?
    } else {
        collapsed
    };
    let first = source.top_of(a).ok_or_else(|| {
        error(
            ErrorCode::NotFound,
            format!("{from} is not a block of its story."),
            "View the outline.",
        )
    })?;
    let last = match end {
        Some((_, z)) => source.top_of(z).ok_or_else(|| {
            error(
                ErrorCode::NotFound,
                "the end of the range is not a block.",
                "View the outline.",
            )
        })?,
        None => source.tops.len().saturating_sub(1),
    };
    if last < first {
        return Err(error(
            ErrorCode::InvalidPayload,
            format!(
                "{} comes before {from}, and a range goes forward.",
                to.unwrap_or_default()
            ),
            "Swap the two refs.",
        ));
    }
    let items = (first..=last)
        .map(|top| Item {
            src: 0,
            top,
            matches: 0,
        })
        .collect();
    Ok(Selected {
        name: "body",
        label: None,
        sources: vec![source],
        items,
        focus: None,
    })
}

/// The tops of one clause: its block, and the blocks under it up to the next clause of the same or a shallower
/// depth. A text that names several clauses is `ambiguous_match` with all of them, and one that names none is
/// `not_found` with the clauses that start the same way.
pub(crate) fn clause_scope(
    parts: &PartsView<'_>,
    mode: Mode,
    text: &str,
) -> Result<Selected, StetError> {
    let index = parts.doc_index()?;
    let Some(found) = index.resolve_for_view(text) else {
        return Err(error(
            ErrorCode::InvalidPayload,
            format!("{text:?} is not a clause number."),
            "Give a clause as `clause 4.2(a)`, `4.2(a)` or `Article III`.",
        ));
    };
    let ref_text = |at: usize| -> String {
        index.clause(at).map_or_else(String::new, |c| {
            let at = parts
                .ref_of(&c.block)
                .map_or_else(|| "?".to_string(), |r| r.to_string());
            format!("{at} ({})", c.heading())
        })
    };
    let at = match found.as_slice() {
        [] => {
            let near = crate::index::near(&index, text);
            let hint = if near.is_empty() {
                "No clause starts the same way.".to_string()
            } else {
                format!("Clauses that start the same way: {}.", near.join(", "))
            };
            return Err(error(
                ErrorCode::NotFound,
                format!("No clause of this document is {text:?}. {hint}"),
                "View the outline for the clauses that exist.",
            ));
        }
        [only] => *only,
        several => {
            let shown: Vec<String> = several.iter().take(8).map(|a| ref_text(*a)).collect();
            let more = several.len().saturating_sub(shown.len());
            let tail = if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            };
            return Err(error(
                ErrorCode::AmbiguousMatch,
                format!(
                    "{text:?} names {} clauses: {}{tail}.",
                    several.len(),
                    shown.join("; ")
                ),
                "View the one you mean as a range of its ref, or add its noun or its parent: `Section 4.2`, `4.2(a)`.",
            ));
        }
    };
    let Some(clause) = index.clause(at) else {
        return Err(error(
            ErrorCode::NotFound,
            format!("No clause of this document is {text:?}."),
            "View the outline for the clauses that exist.",
        ));
    };
    let part = parts.id(clause.block.part()).ok_or_else(|| {
        error(
            ErrorCode::NotFound,
            format!("{text:?} is in a part that is not in this document."),
            "View the outline.",
        )
    })?;
    let source = Source::load(parts, part, mode, false)?;
    let first = source.top_of(clause.entry).ok_or_else(|| {
        error(
            ErrorCode::NotFound,
            format!("{text:?} is not a block of its story."),
            "View the outline.",
        )
    })?;
    let last = match index.ends_at(at) {
        Some(end) => source
            .top_of(end.saturating_sub(1).max(clause.entry))
            .unwrap_or(first),
        None => source.tops.len().saturating_sub(1),
    };
    let name = match clause.story {
        StoryKind::Body => "body",
        StoryKind::Header | StoryKind::Footer => "headers",
        StoryKind::Footnotes | StoryKind::Endnotes => "notes",
        StoryKind::Comments => "comments",
    };
    Ok(Selected {
        name,
        label: Some(format!("clause {}", clause.full)),
        items: (first..=last.max(first))
            .map(|top| Item {
                src: 0,
                top,
                matches: 0,
            })
            .collect(),
        sources: vec![source],
        focus: Some(clause.block.clone()),
    })
}

/// Where the page after a cursor starts: the item after the one that holds the block the cursor names, or
/// for a table that was cut between rows, the same item at the next row.
pub(crate) fn start_after(
    parts: &PartsView<'_>,
    state: &State,
    selected: &Selected,
    cursor: &str,
) -> Result<Start, StetError> {
    let (part, entry) = locate(parts, state, cursor)?;
    let missing = || {
        error(
            ErrorCode::InvalidPayload,
            format!("{cursor} is not in what this view selects."),
            "Start again without --after, or use the same scope as the page that gave the cursor.",
        )
    };
    let (index, item) = selected
        .items
        .iter()
        .enumerate()
        .find(|(_, it)| {
            selected.sources.get(it.src).is_some_and(|s| {
                s.part == part
                    && s.tops
                        .get(it.top)
                        .is_some_and(|t| t.first <= entry && entry < t.end)
            })
        })
        .ok_or_else(missing)?;
    let source = selected.sources.get(item.src).ok_or_else(missing)?;
    let rows = source.rows_of(item.top);
    if rows.is_empty() {
        return Ok(Start {
            item: index + 1,
            row: 0,
        });
    }
    // The row that holds the entry: walk up until the parent is the table.
    let table = source.tops.get(item.top).map_or(0, |t| t.first);
    let mut at = entry;
    while let Some(parent) = source.story.blocks().get(at).and_then(|e| e.parent) {
        if parent as usize == table {
            break;
        }
        at = parent as usize;
    }
    match rows.iter().position(|r| *r == at) {
        Some(k) if k + 1 < rows.len() => Ok(Start {
            item: index,
            row: k + 1,
        }),
        _ => Ok(Start {
            item: index + 1,
            row: 0,
        }),
    }
}
