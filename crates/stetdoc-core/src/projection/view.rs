//! `view`: the pipeline that turns a spec into one page of text (design 4).
//!
//! **Select** the blocks (`select.rs`), **build** the model of a run of them (`build`), **pack** the run into
//! the budget (`page.rs`), **add** the appendix, and **render** with the format. The budget is the estimate
//! of the whole text, so a page is built, rendered and measured, and the number of blocks on it is searched
//! until it is as full as the budget allows. A table that does not fit alone is cut between rows, and the
//! cursor, which is the ref of the last block shown, says which row the next page starts at.

use super::appendix::appendix;
use super::build::{Pick, Selection, StorySel, TopPick, project, story_warnings};
use super::comments_view::comments_view;
use super::formats::{Format, by_id};
use super::ledger_view::ledger_view;
use super::model::{Body, Margin, Outline, OutlineEntry, Output, Projection};
use super::notes::Notes;
use super::outline::{Entry, Full, headings};
use super::page::{estimate, fit, greedy, weight};
use super::select::{
    Item, Selected, Source, Start, clause_scope, find_scope, range_scope, start_after, story_scope,
};
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::{Cursor, FormatId, Mode, Position, Scope, StoryName, View, ViewSpec};
use crate::refs::{BaseToken, BlockRef};
use crate::state::{PartId, PartsView, State};
use crate::wordml::{BlockKind, StoryKind};

/// The tokens that the head of a page costs, about, for the first guess of how many blocks fit.
const OVERHEAD: u32 = 80;

/// What a call of `view` works with.
struct Cx<'a> {
    state: &'a State,
    parts: PartsView<'a>,
    spec: &'a ViewSpec,
    format: &'static dyn Format,
    base: BaseToken,
}

/// A page that is built: its model, its text, and what it says about itself.
struct Page {
    text: String,
    position: Position,
    warnings: Vec<crate::api::view::Warning>,
}

/// One page of a view.
pub(crate) fn view(state: &State, spec: &ViewSpec) -> Result<View, StetError> {
    let cx = Cx {
        state,
        parts: state.parts(),
        spec,
        format: by_id(spec.format.as_str())?,
        base: state.base_token(),
    };
    match &spec.scope {
        Scope::Auto => {
            let selected = story_scope(&cx.parts, spec.mode, StoryName::Body)?;
            if cx.whole_fits(&selected)? {
                cx.blocks(selected)
            } else {
                cx.outline(None)
            }
        }
        Scope::Outline { depth } => cx.outline(depth.map(u32::from)),
        Scope::Story(name) => cx.blocks(story_scope(&cx.parts, spec.mode, *name)?),
        Scope::Range { from, to } => cx.blocks(range_scope(
            &cx.parts,
            state,
            spec.mode,
            from,
            to.as_deref(),
        )?),
        Scope::Find { needle, context } => {
            cx.blocks(find_scope(&cx.parts, spec.mode, needle, *context)?)
        }
        Scope::Clause(text) => cx.blocks(clause_scope(&cx.parts, spec.mode, text)?),
        Scope::Revisions(filter) => ledger_view(cx.parts, spec, filter, cx.format, cx.base),
        Scope::Comments(filter) => comments_view(cx.parts, spec, filter, cx.format, cx.base),
    }
}

impl Cx<'_> {
    fn budget(&self) -> u32 {
        self.spec.budget.0
    }

    fn mode(&self) -> Mode {
        self.spec.mode
    }

    /// The ref of the first block of the entry.
    fn ref_at(&self, source: &Source, entry: usize) -> Option<BlockRef> {
        let e = source.story.blocks().get(entry)?;
        self.parts.ref_of(&source.story.key(e))
    }

    /// The refs of the first and the last block of an item.
    fn item_refs(&self, selected: &Selected, item: &Item) -> (Option<BlockRef>, Option<BlockRef>) {
        let Some(source) = selected.sources.get(item.src) else {
            return (None, None);
        };
        let Some(top) = source.tops.get(item.top) else {
            return (None, None);
        };
        let first = self.ref_at(source, top.first);
        let last = match &top.collapse {
            Some(c) => self.ref_at(source, c.last),
            None => first,
        };
        (first, last)
    }

    fn item_weight(&self, selected: &Selected, item: &Item) -> u32 {
        selected
            .sources
            .get(item.src)
            .and_then(|s| s.sizes(&self.parts, false).ok()?.get(item.top))
            .map_or(4, weight)
    }

    /// True if the whole body is a page that fits the budget, as the weights say and then as the text does.
    fn whole_fits(&self, selected: &Selected) -> Result<bool, StetError> {
        let budget = self.budget();
        if budget == 0 {
            return Ok(true);
        }
        selected.load_sizes(&self.parts, false)?;
        let mut total = 0u32;
        for item in &selected.items {
            total = total.saturating_add(self.item_weight(selected, item));
            if total.saturating_add(OVERHEAD) / 2 > budget {
                return Ok(false);
            }
        }
        let page = self.page(
            selected,
            Start { item: 0, row: 0 },
            selected.items.len(),
            None,
        )?;
        Ok(estimate(&page.text) <= budget)
    }

    /// The page that holds `n` items from `start`, and for a table that is the only item, only `rows` of it.
    fn page(
        &self,
        selected: &Selected,
        start: Start,
        n: usize,
        rows: Option<usize>,
    ) -> Result<Page, StetError> {
        let items = selected
            .items
            .get(start.item..(start.item + n).min(selected.items.len()))
            .unwrap_or_default();
        let mut picks: Vec<(usize, Vec<TopPick>)> = Vec::new();
        let mut last_ref: Option<BlockRef> = None;
        // Notes are for the document as it will be, so a page of `Original` has none.
        let notes = if self.spec.notes && self.mode() != Mode::Original {
            Some(Notes::of(&self.parts)?)
        } else {
            None
        };
        for (k, item) in items.iter().enumerate() {
            let Some(source) = selected.sources.get(item.src) else {
                continue;
            };
            let first_row = if k == 0 { start.row } else { 0 };
            let stop = if k + 1 == items.len() {
                rows.map(|r| first_row + r)
            } else {
                None
            };
            let cut =
                (first_row > 0 || stop.is_some()).then(|| first_row..stop.unwrap_or(usize::MAX));
            let mut pick = TopPick::whole(item.top);
            pick.rows = cut.clone();
            let all_rows = source.rows_of(item.top);
            if first_row > 0
                && let (Some(table), Some(from)) = (
                    source
                        .tops
                        .get(item.top)
                        .and_then(|t| self.ref_at(source, t.first)),
                    all_rows
                        .get(first_row)
                        .and_then(|r| self.ref_at(source, *r)),
                )
            {
                pick.before.push(Margin::Continued { table, from });
            }
            if selected.name == "find" {
                if item.matches > 0 {
                    pick.after.push(Margin::Matches(item.matches));
                }
                let index = start.item + k;
                if let Some(next) = selected.items.get(index + 1)
                    && next.src == item.src
                    && next.top > item.top + 1
                {
                    pick.after.push(Margin::Gap(
                        u32::try_from(next.top - item.top - 1).unwrap_or(u32::MAX),
                    ));
                }
            }
            if let (Some(notes), None) = (&notes, &cut)
                && let Some(top) = source.tops.get(item.top)
                && top.collapse.is_none()
            {
                if let Some(focus) = &selected.focus
                    && source
                        .story
                        .index_of(focus.node())
                        .is_some_and(|e| top.first <= e && e < top.end)
                    && let Some(note) = notes.cited_by(&self.parts, focus)
                {
                    pick.after.push(note);
                }
                for i in top.first..top.end {
                    if let Some(e) = source.story.blocks().get(i)
                        && e.kind == BlockKind::Paragraph
                    {
                        pick.after
                            .extend(notes.mentions(&self.parts, &source.story.key(e)));
                    }
                }
            }
            let (_, last) = self.item_refs(selected, item);
            last_ref = match (&cut, stop) {
                (Some(_), Some(end)) => {
                    let shown = end.min(all_rows.len());
                    shown
                        .checked_sub(1)
                        .and_then(|i| all_rows.get(i))
                        .and_then(|r| self.ref_at(source, *r))
                        .or(last)
                }
                _ => last,
            };
            match picks.last_mut() {
                Some((src, tops)) if *src == item.src => tops.push(pick),
                _ => picks.push((item.src, vec![pick])),
            }
        }
        let selection = Selection {
            stories: picks
                .into_iter()
                .filter_map(|(src, tops)| {
                    let source = selected.sources.get(src)?;
                    Some((
                        source.part,
                        StorySel::Pick(Pick {
                            tops,
                            expanded: source.expanded,
                        }),
                    ))
                })
                .collect(),
        };
        let mut projection = project(
            self.parts,
            self.base,
            self.mode(),
            Output::View {
                name: selected.name.into(),
            },
            &selection,
        )?;
        let position = self.position(selected, start, items, last_ref, rows);
        projection.head.position = Some(position.clone());
        if self.spec.notes {
            projection.appendix = appendix(self.parts, &self.base, self.mode(), &projection)?;
        }
        if self.format.id() == FormatId::JSON {
            super::render::fill_final_text(self.state, &mut projection);
        }
        let warnings = projection.head.warnings.clone();
        Ok(Page {
            text: self.render(&projection),
            position,
            warnings,
        })
    }

    fn render(&self, projection: &Projection) -> String {
        self.format
            .render(projection)
            .files
            .into_iter()
            .next()
            .map(|(_, text)| text)
            .unwrap_or_default()
    }

    /// Where a page is in what it selects.
    fn position(
        &self,
        selected: &Selected,
        start: Start,
        items: &[Item],
        last_ref: Option<BlockRef>,
        rows: Option<usize>,
    ) -> Position {
        let first = items.first().and_then(|i| self.item_refs(selected, i).0);
        let whole_first = selected
            .items
            .first()
            .and_then(|i| self.item_refs(selected, i).0);
        let whole_last = selected
            .items
            .last()
            .and_then(|i| self.item_refs(selected, i).1);
        let scope = selected
            .label
            .clone()
            .unwrap_or_else(|| match (whole_first, whole_last) {
                (Some(a), Some(b)) => format!("{a}..{b}"),
                _ => selected.name.to_string(),
            });
        let after = start.item + items.len();
        let mut remaining: u32 = selected
            .items
            .iter()
            .skip(after)
            .map(|i| self.item_weight(selected, i))
            .fold(0, u32::saturating_add);
        let mut shown = u32::try_from(items.len()).unwrap_or(u32::MAX);
        let total = u32::try_from(selected.items.len()).unwrap_or(u32::MAX);
        if let (Some(item), Some(r)) = (items.last(), rows) {
            // A table cut between rows: the rest of it is still to come.
            let all = selected
                .sources
                .get(item.src)
                .map_or(0, |s| s.rows_of(item.top).len());
            let done = if items.len() == 1 { start.row + r } else { r };
            if done < all {
                let left = u32::try_from(all - done).unwrap_or(u32::MAX);
                let w = self.item_weight(selected, item);
                remaining = remaining.saturating_add(
                    w.saturating_mul(left) / u32::try_from(all).unwrap_or(1).max(1),
                );
                shown = shown.saturating_sub(0);
            }
        }
        Position {
            scope,
            first: first.map(|r| r.to_string()),
            last: last_ref.map(|r| r.to_string()),
            shown,
            total,
            remaining_tokens: remaining,
        }
    }

    /// A page of blocks, from the cursor if there is one.
    fn blocks(&self, selected: Selected) -> Result<View, StetError> {
        let start = match &self.spec.after {
            Some(c) => start_after(&self.parts, self.state, &selected, c.as_str())?,
            None => Start { item: 0, row: 0 },
        };
        let budget = self.budget();
        selected.load_sizes(
            &self.parts,
            self.spec.notes && self.mode() != Mode::Original,
        )?;
        let left = selected.items.len().saturating_sub(start.item);
        if left == 0 {
            let page = self.page(&selected, start, 0, None)?;
            return Ok(self.finish(page, None));
        }
        let weights = selected
            .items
            .iter()
            .skip(start.item)
            .map(|i| self.item_weight(&selected, i));
        let guess = greedy(weights, budget, OVERHEAD);
        let mut n = fit(left, budget, guess, &mut |n| {
            Ok(estimate(&self.page(&selected, start, n, None)?.text))
        })?;
        let mut rows = None;
        if n == 1
            && budget != 0
            && let Some(first) = selected.items.get(start.item)
            && let Some(source) = selected.sources.get(first.src)
        {
            let all = source.rows_of(first.top).len();
            let left_rows = all.saturating_sub(start.row);
            let whole = self.page(&selected, start, 1, None)?;
            if estimate(&whole.text) > budget && left_rows > 1 {
                let k = fit(left_rows, budget, left_rows / 2 + 1, &mut |k| {
                    Ok(estimate(&self.page(&selected, start, 1, Some(k))?.text))
                })?;
                if k < left_rows {
                    rows = Some(k);
                }
            }
        }
        n = n.max(1);
        let page = self.page(&selected, start, n, rows)?;
        let more = start.item + n < selected.items.len() || rows.is_some();
        let next = match (&page.position.last, more) {
            (Some(last), true) => Some(Cursor::new(last.clone())),
            _ => None,
        };
        Ok(self.finish(page, next))
    }

    fn finish(&self, page: Page, next: Option<Cursor>) -> View {
        View {
            base: self.base.to_string(),
            mode: self.mode(),
            tokens: estimate(&page.text),
            text: page.text,
            position: page.position,
            next,
            warnings: page.warnings,
        }
    }

    /// The outline of the body, to a depth, or to the deepest level that fits the budget.
    fn outline(&self, depth: Option<u32>) -> Result<View, StetError> {
        let list = self.parts.story_specs()?;
        let part = list
            .0
            .iter()
            .find(|s| s.kind == StoryKind::Body)
            .and_then(|s| self.parts.id(&s.part))
            .ok_or_else(|| StetError::new(ErrorCode::NotFound, "the document has no body."))?;
        let source = Source::load(&self.parts, part, self.mode(), false)?;
        let tree = self.parts.tree(part)?;
        let mode = self.mode();
        let first_text = |t: usize| -> String {
            let Some(top) = source.tops.get(t) else {
                return String::new();
            };
            (top.first..top.end)
                .filter(|i| {
                    source.story.blocks().get(*i).map(|e| e.kind) == Some(BlockKind::Paragraph)
                })
                .filter_map(|i| source.story.paragraph(tree, i))
                .map(|p| p.text(mode))
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .find(|t| !t.is_empty())
                .unwrap_or_default()
        };
        let bold = || self.parts.bold_lines(part, mode, false);
        let scan = self.parts.scan(part, mode, false)?;
        let full = headings(&scan, &source.tops, &bold, &first_text)?;
        let deepest = full.deepest();
        let budget = self.budget();
        let measure = |d: u32, from: usize, n: Option<usize>| -> Result<(Page, usize), StetError> {
            self.outline_page(&full, &source, part, d, from, n)
        };
        let chosen = match depth {
            Some(d) => d.clamp(1, deepest),
            None => {
                let mut pick = 1;
                for d in (1..=deepest).rev() {
                    let (page, _) = measure(d, 0, None)?;
                    if budget == 0 || estimate(&page.text) <= budget {
                        pick = d;
                        break;
                    }
                }
                pick
            }
        };
        let shown = full.shown(chosen);
        let start = match &self.spec.after {
            Some(c) => {
                let (_, entry) = super::select::locate(&self.parts, self.state, c.as_str())?;
                shown
                    .iter()
                    .position(|(e, _)| e.entry > entry)
                    .unwrap_or(shown.len())
            }
            None => 0,
        };
        let left = shown.len().saturating_sub(start);
        let guess = greedy(std::iter::repeat_n(30u32, left), budget, OVERHEAD);
        let n = if left == 0 {
            0
        } else {
            fit(left, budget, guess, &mut |n| {
                Ok(estimate(&measure(chosen, start, Some(n))?.0.text))
            })?
        };
        let (page, _) = measure(chosen, start, Some(n))?;
        let next = (start + n < shown.len())
            .then(|| page.position.last.clone())
            .flatten()
            .map(Cursor::new);
        Ok(self.finish(page, next))
    }

    /// The page of an outline: the entries `from..from + n` of those down to `depth`.
    fn outline_page(
        &self,
        full: &Full,
        source: &Source,
        part: PartId,
        depth: u32,
        from: usize,
        n: Option<usize>,
    ) -> Result<(Page, usize), StetError> {
        let shown = full.shown(depth);
        let end = n.map_or(shown.len(), |n| (from + n).min(shown.len()));
        let slice = shown.get(from.min(end)..end).unwrap_or_default();
        let entries: Vec<OutlineEntry> = slice
            .iter()
            .map(|(e, hidden)| self.outline_entry(source, e, *hidden))
            .collect();
        let first = entries.iter().find_map(|e| e.r#ref);
        let last = entries.iter().rev().find_map(|e| e.r#ref);
        let mut projection = project(
            self.parts,
            self.base,
            self.mode(),
            Output::View {
                name: "outline".into(),
            },
            &Selection {
                stories: Vec::new(),
            },
        )?;
        projection
            .head
            .warnings
            .extend(story_warnings(self.parts, self.mode(), part)?);
        let remaining = u32::try_from(shown.len().saturating_sub(end))
            .unwrap_or(u32::MAX)
            .saturating_mul(30);
        let position = Position {
            scope: "outline".to_string(),
            first: first.map(|r| r.to_string()),
            last: last.map(|r| r.to_string()),
            shown: u32::try_from(entries.len()).unwrap_or(u32::MAX),
            total: u32::try_from(shown.len()).unwrap_or(u32::MAX),
            remaining_tokens: remaining,
        };
        projection.head.position = Some(position.clone());
        projection.body = Body::Outline(Outline {
            source: full.source,
            depth,
            total: u32::try_from(full.entries.len()).unwrap_or(u32::MAX),
            entries,
        });
        let warnings = projection.head.warnings.clone();
        Ok((
            Page {
                text: self.render(&projection),
                position,
                warnings,
            },
            end - from.min(end),
        ))
    }

    fn outline_entry(&self, source: &Source, e: &Entry, hidden: u32) -> OutlineEntry {
        let tops = &source.tops;
        let first = tops.get(e.top).and_then(|t| self.ref_at(source, t.first));
        let last = e
            .end
            .checked_sub(1)
            .and_then(|i| tops.get(i))
            .and_then(|t| match &t.collapse {
                Some(c) => self.ref_at(source, c.last),
                None => self.ref_at(source, t.first),
            });
        OutlineEntry {
            r#ref: self.ref_at(source, e.entry),
            level: e.level,
            label: e.label.clone(),
            text: e.text.clone(),
            blocks: e.blocks,
            chars: e.chars,
            first,
            last,
            hidden,
        }
    }
}
