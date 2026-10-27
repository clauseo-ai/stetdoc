//! The compact page of a `Find` with no context: one line for each paragraph that matches, with its ref, the clause it
//! is in, and the text around the match, under a line for each outermost clause the hits are in. A page is a run of the
//! items, as for a find with context, so the cursor is the ref of the last paragraph shown and paging works the same.

use super::super::build::{Selection, project};
use super::super::model::{Body, FindHit, HitList, Output, Projection, Target, TermCount, Tip};
use super::super::page::{estimate, fit, greedy};
use super::super::place::{Placer, heading_text};
use super::super::select::{Selected, Source, locate, start_after};
use super::super::select_find::Found;
use super::{Cx, OVERHEAD};
use crate::api::error::StetError;
use crate::api::view::{Cursor, Mode, Position, View};
use crate::wordml::BlockKind;

/// The most tips a page gives.
const TIPS: usize = 3;

/// The tokens a hit's line costs beyond its text, about.
const LINE: u32 = 10;

/// How many characters of each other cell of its row a hit in a table shows.
const CELL: usize = 40;

/// How many other paragraphs of its row a hit in a table shows.
const CELLS: usize = 8;

/// One hit, ready for a page: the item it belongs to, its line, and the tip it gives if it is a clause heading.
struct Line {
    item: usize,
    entry: usize,
    hit: FindHit,
    tip: Option<Tip>,
}

impl Cx<'_> {
    /// A page of the paragraphs a find matched, from the cursor if there is one. A page is a run of the lines, so a table
    /// with many matching cells is paged like any other run of paragraphs.
    pub(super) fn hits(&self, selected: &Selected, found: &Found) -> Result<View, StetError> {
        let lines = self.lines(selected, found)?;
        let start = match &self.spec.after {
            Some(c) => {
                let at = start_after(&self.parts, self.state, selected, c.as_str())?;
                let (_, entry) = locate(&self.parts, self.state, c.as_str())?;
                // A cursor in a table names the last paragraph listed, and the lines after it in the table come next.
                lines
                    .iter()
                    .position(|l| {
                        l.item > at.item || (l.item == at.item && (at.row == 0 || l.entry > entry))
                    })
                    .unwrap_or(lines.len())
            }
            None => 0,
        };
        let weights: Vec<u32> = lines
            .iter()
            .map(|line| {
                let chars: usize = line.hit.text.chars().count()
                    + line
                        .hit
                        .row
                        .iter()
                        .map(|c| c.text.chars().count() + 8)
                        .sum::<usize>();
                u32::try_from(chars.div_ceil(4))
                    .unwrap_or(u32::MAX)
                    .saturating_add(LINE)
            })
            .collect();
        let head = project(
            self.parts,
            self.base,
            self.mode(),
            Output::View {
                name: selected.name.into(),
            },
            &Selection {
                stories: Vec::new(),
            },
        )?;
        let page = |n: usize| self.hits_page(&head, selected, found, &lines, &weights, start, n);
        let budget = self.budget();
        let left = lines.len().saturating_sub(start);
        let guess = greedy(weights.iter().skip(start).copied(), budget, OVERHEAD);
        let n = if left == 0 {
            0
        } else {
            fit(left, budget, guess, &mut |n| Ok(estimate(&page(n).0)))?.max(1)
        };
        let (text, position) = page(n);
        let next = (start + n < lines.len())
            .then(|| position.last.clone())
            .flatten()
            .map(Cursor::new);
        Ok(View {
            base: self.base.to_string(),
            mode: self.mode(),
            tokens: estimate(&text),
            text,
            position,
            next,
            warnings: head.head.warnings.clone(),
        })
    }

    /// The line of every hit, with the clause it is in from the index, or the heading it is under.
    fn lines(&self, selected: &Selected, found: &Found) -> Result<Vec<Line>, StetError> {
        let index = self.parts.doc_index()?;
        let mut placer = Placer::new(self.parts, &index)?;
        let mut out = Vec::with_capacity(found.hits.len());
        for hit in &found.hits {
            let Some(source) = selected
                .items
                .get(hit.item)
                .and_then(|item| selected.sources.get(item.src))
            else {
                continue;
            };
            let Some(entry) = source.story.blocks().get(hit.entry) else {
                continue;
            };
            let key = source.story.key(entry);
            let Some(r#ref) = self.parts.ref_of(&key) else {
                continue;
            };
            let place = placer.place(source.part, &source.story, hit.entry)?;
            let clause = place.clause;
            let under = match (place.at, place.heading) {
                (Some(at), _) => index
                    .clause(index.ancestors(at).last().copied().unwrap_or(at))
                    .and_then(|c| {
                        Some(Target {
                            r#ref: self.parts.ref_of(&c.block)?,
                            text: c.heading().into(),
                        })
                    }),
                (None, Some(h)) => match self.ref_at(source, h) {
                    Some(r#ref) => Some(Target {
                        r#ref,
                        text: heading_text(&self.parts, source.part, &source.story, h)?.into(),
                    }),
                    None => None,
                },
                (None, None) => None,
            };
            let tip = if hit.short {
                index.clause_of(&key).map(|c| Tip {
                    r#ref,
                    clause: match c.noun {
                        Some(noun) => format!("{} {}", noun.to_lowercase(), c.full),
                        None => c.full.clone(),
                    }
                    .into(),
                    text: c.heading().into(),
                })
            } else {
                None
            };
            let terms = found
                .needles
                .iter()
                .zip(&hit.counts)
                .filter(|(_, n)| **n > 0)
                .map(|(term, count)| TermCount {
                    term: term.as_str().into(),
                    count: *count,
                })
                .collect();
            out.push(Line {
                item: hit.item,
                entry: hit.entry,
                hit: FindHit {
                    r#ref,
                    under,
                    // A bare label such as `1` keeps its noun, so that Schedule 1 does not read as clause 1.
                    clause: clause.map(|c| match c.noun {
                        Some(noun) if !c.full.contains(['.', '(']) => {
                            format!("{noun} {}", c.full).into()
                        }
                        _ => c.full.as_str().into(),
                    }),
                    text: hit.snippet.text.as_str().into(),
                    cut_before: hit.snippet.cut_before,
                    cut_after: hit.snippet.cut_after,
                    terms,
                    deleted_only: hit.deleted_only,
                    row: self.row_of(source, hit.entry)?,
                },
                tip,
            });
        }
        Ok(out)
    }

    /// The other paragraphs of the row that holds the paragraph at `entry`, each cut to `CELL` characters, or none when
    /// it is not in a table.
    fn row_of(&self, source: &Source, entry: usize) -> Result<Vec<Target>, StetError> {
        let blocks = source.story.blocks();
        let Some(row) = blocks.get(entry).and_then(|e| e.parent) else {
            return Ok(Vec::new());
        };
        let row = row as usize;
        let table = blocks.get(row).and_then(|r| r.parent);
        let tree = self.parts.tree(source.part)?;
        let mode = match self.mode() {
            Mode::Redline => Mode::Final,
            mode => mode,
        };
        let mut out = Vec::new();
        for (i, block) in blocks.iter().enumerate().skip(row + 1) {
            if block.parent.is_none() || (block.kind == BlockKind::Row && block.parent == table) {
                break;
            }
            if i == entry
                || block.kind != BlockKind::Paragraph
                || block.parent != u32::try_from(row).ok()
            {
                continue;
            }
            let Some(p) = source.story.paragraph(tree, i) else {
                continue;
            };
            let words = p.text(mode);
            let text = words
                .split(|c: char| {
                    c.is_whitespace() || matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}')
                })
                .filter(|w| !w.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let Some(r#ref) = self.ref_at(source, i) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            if out.len() == CELLS {
                break;
            }
            let text = if text.chars().count() > CELL {
                let cut: String = text.chars().take(CELL - 1).collect();
                format!("{}…", cut.trim_end())
            } else {
                text
            };
            out.push(Target {
                r#ref,
                text: text.into(),
            });
        }
        Ok(out)
    }

    /// The text and the position of the page that holds the items `start..start + n`.
    #[expect(
        clippy::too_many_arguments,
        reason = "the parts of one page, all borrowed from the caller"
    )]
    fn hits_page(
        &self,
        head: &Projection,
        selected: &Selected,
        found: &Found,
        lines: &[Line],
        weights: &[u32],
        start: usize,
        n: usize,
    ) -> (String, Position) {
        let end = (start + n).min(lines.len());
        let shown: Vec<&Line> = lines
            .get(start.min(end)..end)
            .unwrap_or_default()
            .iter()
            .collect();
        let mut scope = selected.label.clone().unwrap_or_default();
        if end > start && (start > 0 || end < lines.len()) {
            scope.push_str(&format!(", showing {}..{}", start + 1, end));
        }
        let position = Position {
            scope,
            first: shown.first().map(|l| l.hit.r#ref.to_string()),
            last: shown.last().map(|l| l.hit.r#ref.to_string()),
            shown: u32::try_from(end.saturating_sub(start)).unwrap_or(u32::MAX),
            total: u32::try_from(lines.len()).unwrap_or(u32::MAX),
            remaining_tokens: weights
                .iter()
                .skip(end)
                .fold(0u32, |a, w| a.saturating_add(*w)),
        };
        let mut projection = Projection {
            head: head.head.clone(),
            body: Body::Hits(HitList {
                needles: u32::try_from(found.needles.len()).unwrap_or(u32::MAX),
                hits: shown.iter().map(|l| l.hit.clone()).collect(),
                tips: shown
                    .iter()
                    .filter_map(|l| l.tip.clone())
                    .take(TIPS)
                    .collect(),
            }),
            appendix: Vec::new(),
        };
        projection.head.position = Some(position.clone());
        (self.render(&projection), position)
    }
}
