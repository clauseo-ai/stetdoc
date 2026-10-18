//! One pass over a story for what a view needs before it builds a page: how big each top is, and for each
//! paragraph the facts that decide whether it is a heading (design 4.3 and 4.4).
//!
//! The pass is a derived value of `state`, keyed by the story and the mode, so the second page of a view
//! and the outline of the same story cost nothing. It reads each paragraph's text map once, and the
//! cascade only for what a heading test asks: the outline level of a paragraph, the level of its list, and,
//! for a short paragraph outside a table, whether every run is bold.

use super::cx::{StoryCx, is_sentinel};
use super::inlines::{is_textual, slices};
use super::layout::Top;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::state::{Dep, PartId, PartsView, RunKey};
use crate::wordml::{BlockKind, Row as WRow, SpanTree, Story, StoryKind};
use std::sync::Arc;

/// How big a top is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TopStat {
    /// The characters of text in the mode.
    pub chars: u32,
    /// The lines the top is rendered as: one for each paragraph and row, and two for a table.
    pub lines: u32,
}

/// The facts of one paragraph that a heading test reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ParaStat {
    /// The top it is in.
    pub top: u32,
    /// Its entry.
    pub entry: u32,
    /// The characters of its text in the mode.
    pub chars: u32,
    /// It is in a table.
    pub in_table: bool,
    /// The effective outline level of its style, 0 to 8.
    pub outline_level: Option<u8>,
    /// It is numbered, at the first level of its list.
    pub list0: bool,
    /// The number Word shows for it.
    pub label: Option<Box<str>>,
    /// The first hundred characters of its text, with runs of whitespace as one space, kept for a paragraph
    /// that can be a heading and empty for the rest.
    pub text: Box<str>,
}

/// What the pass found in a story.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Scan {
    /// The size of each top, in the order of the story's tops.
    pub tops: Vec<TopStat>,
    /// Each paragraph that exists in the mode, in order.
    pub paras: Vec<ParaStat>,
}

/// How many characters of a paragraph are kept for a heading's text.
const KEPT: usize = 100;

/// True if a paragraph shows nothing in `mode`: no character, and no field, whose markup the view writes even when its result is
/// empty. A paragraph that shows nothing and joins the next one is left out of the view, as the join leaves nothing of it.
pub(super) fn shows_nothing(map: &crate::wordml::TextMap, mode: Mode) -> bool {
    map.len(mode) == 0
        && !map
            .items()
            .iter()
            .any(|i| i.kind == crate::wordml::ItemKind::FieldBegin && i.vis.in_mode(mode))
}

/// A paragraph with at most this many characters can be a heading.
pub(crate) const SHORT: u32 = 100;

fn mode_code(mode: Mode) -> u64 {
    match mode {
        Mode::Final => 0,
        Mode::Original => 1,
        Mode::Redline => 2,
    }
}

impl PartsView<'_> {
    /// The scan of the story at `part` in `mode`, over the tops with or without the collapsed fields, cached
    /// until the story, the styles or the numbering change.
    pub(crate) fn scan(
        &self,
        part: PartId,
        mode: Mode,
        expanded: bool,
    ) -> Result<Arc<Scan>, StetError> {
        let mut deps = self.numbers_deps(part)?;
        deps.extend(self.cascade_deps()?);
        deps.push(Dep::Part(part));
        deps.push(Dep::Package);
        let build = |_: Option<&Scan>| -> Result<Scan, StetError> {
            let tops = self.tops(part, mode, expanded)?;
            let facts = self.doc_facts()?;
            let mut cx = StoryCx::new(*self, mode, facts, part)?;
            cx.scan(&tops)
        };
        let variant = (u64::from(part.number()) * 4 + mode_code(mode)) * 2 + u64::from(expanded);
        self.derived(variant, &deps, &build)
    }
}

impl PartsView<'_> {
    /// The size of each top of the story at `part`, which is all a page of a range or a story needs of the
    /// scan, for the position line and the first guess of how many blocks fit. The lengths of the paragraphs
    /// are the ones that the index read in its one pass over the text, so a document that has a clause table
    /// is not read twice, and the sizes are those of `scan` (law P7), without the facts of a heading test,
    /// which ask the cascade about each paragraph and which only an outline wants.
    pub(crate) fn top_sizes(
        &self,
        part: PartId,
        mode: Mode,
        expanded: bool,
    ) -> Result<Arc<Vec<TopStat>>, StetError> {
        let mut deps = self.numbers_deps(part)?;
        deps.push(Dep::Part(part));
        deps.push(Dep::Package);
        let build = |_: Option<&Vec<TopStat>>| -> Result<Vec<TopStat>, StetError> {
            let tops = self.tops(part, mode, expanded)?;
            let facts = self.story_facts(part)?;
            let story = self.story(part)?;
            let tree = self.tree(part)?;
            Ok(sizes_of(&story, tree, mode, &tops, &facts.lens))
        };
        let variant = (u64::from(part.number()) * 4 + mode_code(mode)) * 2 + u64::from(expanded);
        self.derived(variant, &deps, &build)
    }
}

/// The size of each top, from the length of each paragraph in each mode (`StoryFacts::lens`). It counts what
/// `StoryCx::scan_top` counts, in the same order of tests, so that the two agree (law P9 checks that they do).
fn sizes_of(
    story: &Story,
    tree: &SpanTree,
    mode: Mode,
    tops: &[Top],
    lens: &[[u32; 3]],
) -> Vec<TopStat> {
    let mut row_gone: Vec<Option<bool>> = vec![None; story.blocks().len()];
    let column = mode_code(mode) as usize;
    tops.iter()
        .map(|top| {
            if top.collapse.is_some() {
                return TopStat { chars: 0, lines: 1 };
            }
            let mut stat = TopStat::default();
            if story.blocks().get(top.first).map(|e| e.kind) == Some(BlockKind::Table) {
                stat.lines += 2;
            }
            for i in top.first..top.end {
                let Some(entry) = story.blocks().get(i).copied() else {
                    continue;
                };
                if entry.kind == BlockKind::Row {
                    if !row_is_gone(story, tree, mode, i, &mut row_gone) {
                        stat.lines += 1;
                    }
                    continue;
                }
                if entry.kind != BlockKind::Paragraph {
                    continue;
                }
                let in_table = entry.parent.is_some();
                if let Some(row) = entry.parent
                    && row_is_gone(story, tree, mode, row as usize, &mut row_gone)
                {
                    continue;
                }
                let Some(p) = story.paragraph(tree, i) else {
                    continue;
                };
                let chars = lens
                    .get(i)
                    .and_then(|l| l.get(column))
                    .copied()
                    .unwrap_or(0);
                if !in_table && p.joins_next(mode) && chars == 0 && shows_nothing(&p.map(), mode) {
                    continue;
                }
                stat.chars = stat.chars.saturating_add(chars);
                stat.lines += 1;
            }
            stat
        })
        .collect()
}

/// True if the row at entry `i` does not exist in the mode: deleted in `Final`, inserted in `Original`.
fn row_is_gone(
    story: &Story,
    tree: &SpanTree,
    mode: Mode,
    i: usize,
    cache: &mut [Option<bool>],
) -> bool {
    if let Some(Some(known)) = cache.get(i) {
        return *known;
    }
    let gone = story
        .blocks()
        .get(i)
        .and_then(|e| WRow::new(tree, e.node))
        .is_some_and(|r| r.revs().gone_in(mode));
    if let Some(slot) = cache.get_mut(i) {
        *slot = Some(gone);
    }
    gone
}

impl PartsView<'_> {
    /// For each paragraph of the scan, in order: it is a short line outside a table whose every run with
    /// text is effectively bold. It asks the cascade about each run, which a page never needs, so it is
    /// its own value, built when an outline has no better source of headings.
    pub(crate) fn bold_lines(
        &self,
        part: PartId,
        mode: Mode,
        expanded: bool,
    ) -> Result<Arc<Vec<bool>>, StetError> {
        let mut deps = self.numbers_deps(part)?;
        deps.extend(self.cascade_deps()?);
        deps.push(Dep::Part(part));
        deps.push(Dep::Package);
        let build = |_: Option<&Vec<bool>>| -> Result<Vec<bool>, StetError> {
            let scan = self.scan(part, mode, expanded)?;
            let facts = self.doc_facts()?;
            let mut cx = StoryCx::new(*self, mode, facts, part)?;
            cx.bold(&scan)
        };
        let variant = (u64::from(part.number()) * 4 + mode_code(mode)) * 2 + u64::from(expanded);
        self.derived(variant, &deps, &build)
    }
}

/// Collapses runs of whitespace to one space and trims, keeping at most `KEPT` characters.
fn collapse(text: &str) -> String {
    let mut out = String::new();
    let mut pending = false;
    for c in text.chars().filter(|c| !is_sentinel(*c)) {
        if c.is_whitespace() {
            pending = !out.is_empty();
            continue;
        }
        if pending {
            out.push(' ');
            pending = false;
        }
        out.push(c);
        if out.chars().count() >= KEPT {
            break;
        }
    }
    out
}

impl StoryCx<'_> {
    /// Reads every paragraph of the story once.
    fn scan(&mut self, tops: &[Top]) -> Result<Scan, StetError> {
        let mut out = Scan {
            tops: Vec::with_capacity(tops.len()),
            paras: Vec::new(),
        };
        let mut row_gone: Vec<Option<bool>> = vec![None; self.story.blocks().len()];
        for (t, top) in tops.iter().enumerate() {
            let stat = self.scan_top(t, top, &mut row_gone, &mut out.paras)?;
            out.tops.push(stat);
        }
        Ok(out)
    }

    /// The size of the top `t`, and, in the body, the facts of each of its paragraphs that a heading test reads.
    fn scan_top(
        &mut self,
        t: usize,
        top: &Top,
        row_gone: &mut [Option<bool>],
        paras: &mut Vec<ParaStat>,
    ) -> Result<TopStat, StetError> {
        let story = self.story.clone();
        let body = self.kind == StoryKind::Body;
        let mut stat = TopStat::default();
        if top.collapse.is_some() {
            return Ok(TopStat { chars: 0, lines: 1 });
        }
        if story.blocks().get(top.first).map(|e| e.kind) == Some(BlockKind::Table) {
            stat.lines += 2;
        }
        for i in top.first..top.end {
            let Some(entry) = story.blocks().get(i).copied() else {
                continue;
            };
            if entry.kind == BlockKind::Row {
                if !self.row_gone(i, row_gone) {
                    stat.lines += 1;
                }
                continue;
            }
            if entry.kind != BlockKind::Paragraph {
                continue;
            }
            let in_table = entry.parent.is_some();
            if let Some(row) = entry.parent
                && self.row_gone(row as usize, row_gone)
            {
                continue;
            }
            let Some(p) = story.paragraph(self.tree, i) else {
                continue;
            };
            let map = p.map();
            let chars = u32::try_from(map.len(self.mode)).unwrap_or(u32::MAX);
            if !in_table && p.joins_next(self.mode) && shows_nothing(&map, self.mode) {
                continue;
            }
            stat.chars = stat.chars.saturating_add(chars);
            stat.lines += 1;
            if !body {
                continue;
            }
            let site = self.sites.site_for(entry.node, None)?;
            let outline_level = self.cascade.outline_level(&site);
            let list0 = site.level.is_some_and(|l| l.ilvl == 0);
            let label = self
                .sites
                .numbers()
                .and_then(|n| n.clause_number(entry.node))
                .map(Into::into);
            let candidate = outline_level.is_some() || list0 || (chars <= SHORT && !in_table);
            let text: Box<str> = if candidate {
                collapse(&map.text(self.mode)).into()
            } else {
                Box::default()
            };
            paras.push(ParaStat {
                top: u32::try_from(t).unwrap_or(u32::MAX),
                entry: u32::try_from(i).unwrap_or(u32::MAX),
                chars,
                in_table,
                outline_level,
                list0,
                label,
                text,
            });
        }
        Ok(stat)
    }

    /// For each paragraph of the scan: whether it is a short line whose runs are all bold.
    fn bold(&mut self, scan: &Scan) -> Result<Vec<bool>, StetError> {
        let story = self.story.clone();
        let mut out = Vec::with_capacity(scan.paras.len());
        for p in &scan.paras {
            let candidate = p.chars <= SHORT && !p.in_table && !p.text.is_empty();
            let found =
                if candidate && let Some(para) = story.paragraph(self.tree, p.entry as usize) {
                    let map = para.map();
                    let redline = map.text(Mode::Redline);
                    let pieces = slices(&redline, map.items());
                    let node = story.blocks().get(p.entry as usize).map(|e| e.node);
                    match node {
                        Some(node) => self.runs_all_bold(node, &map, &pieces)?,
                        None => false,
                    }
                } else {
                    false
                };
            out.push(found);
        }
        Ok(out)
    }

    /// True if every run with text in the paragraph is effectively bold, and there is one.
    fn runs_all_bold(
        &mut self,
        node: crate::wordml::NodeId,
        map: &crate::wordml::TextMap,
        pieces: &[&str],
    ) -> Result<bool, StetError> {
        let mut last = None;
        let mut any = false;
        for (item, piece) in map.items().iter().zip(pieces) {
            if !is_textual(item.kind) || !item.vis.in_mode(self.mode) || piece.trim().is_empty() {
                continue;
            }
            if item.run.is_some() && item.run == last {
                continue;
            }
            last = item.run;
            let key = RunKey {
                item,
                complex_script: false,
            };
            let site = self.sites.site_for(node, Some(key))?;
            if !self.cascade.effective(&site).on("b") {
                return Ok(false);
            }
            any = true;
        }
        Ok(any)
    }

    /// True if the row at entry `i` does not exist in the mode: deleted in `Final`, inserted in `Original`.
    fn row_gone(&self, i: usize, cache: &mut [Option<bool>]) -> bool {
        row_is_gone(&self.story, self.tree, self.mode, i, cache)
    }
}
