//! The compact page of a `Find` with no context: one line for each paragraph that matches, with its ref, the clause it
//! is in, and the text around the match, under a line for each outermost clause the hits are in. A page is a run of the
//! items, as for a find with context, so the cursor is the ref of the last paragraph shown and paging works the same.

use super::super::build::{Selection, project};
use super::super::model::{Body, FindHit, HitList, Output, Projection, Target, TermCount, Tip};
use super::super::page::{estimate, fit, greedy};
use super::super::select::{Selected, start_after};
use super::super::select_find::Found;
use super::{Cx, OVERHEAD};
use crate::api::error::StetError;
use crate::api::view::{Cursor, Position, View};

/// The most tips a page gives.
const TIPS: usize = 3;

/// The tokens a hit's line costs beyond its text, about.
const LINE: u32 = 10;

/// One hit, ready for a page: the item it belongs to, its line, and the tip it gives if it is a clause heading.
struct Line {
    item: usize,
    hit: FindHit,
    tip: Option<Tip>,
}

impl Cx<'_> {
    /// A page of the paragraphs a find matched, from the cursor if there is one.
    pub(super) fn hits(&self, selected: &Selected, found: &Found) -> Result<View, StetError> {
        let start = match &self.spec.after {
            Some(c) => {
                let at = start_after(&self.parts, self.state, selected, c.as_str())?;
                // A cursor in a table names one of its rows, and the whole table was listed.
                at.item + usize::from(at.row > 0)
            }
            None => 0,
        };
        let lines = self.lines(selected, found)?;
        let mut weights = vec![0u32; selected.items.len()];
        for line in &lines {
            if let Some(w) = weights.get_mut(line.item) {
                let chars = u32::try_from(line.hit.text.chars().count()).unwrap_or(u32::MAX);
                *w = w.saturating_add(chars.div_ceil(4).saturating_add(LINE));
            }
        }
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
        let left = selected.items.len().saturating_sub(start);
        let guess = greedy(weights.iter().skip(start).copied(), budget, OVERHEAD);
        let n = if left == 0 {
            0
        } else {
            fit(left, budget, guess, &mut |n| Ok(estimate(&page(n).0)))?.max(1)
        };
        let (text, position) = page(n);
        let next = (start + n < selected.items.len())
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

    /// The line of every hit, with the clause it is in from the index.
    fn lines(&self, selected: &Selected, found: &Found) -> Result<Vec<Line>, StetError> {
        let index = self.parts.doc_index()?;
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
            let at = index.context_at(source.story.part(), hit.entry);
            let clause = at.and_then(|at| index.clause(at));
            let under = at
                .map(|at| index.ancestors(at).last().copied().unwrap_or(at))
                .and_then(|top| index.clause(top))
                .and_then(|c| {
                    Some(Target {
                        r#ref: self.parts.ref_of(&c.block)?,
                        text: c.heading().into(),
                    })
                });
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
                hit: FindHit {
                    r#ref,
                    under,
                    clause: clause.map(|c| c.full.as_str().into()),
                    text: hit.snippet.text.as_str().into(),
                    cut_before: hit.snippet.cut_before,
                    cut_after: hit.snippet.cut_after,
                    terms,
                    deleted_only: hit.deleted_only,
                },
                tip,
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
        let end = (start + n).min(selected.items.len());
        let shown: Vec<&Line> = lines
            .iter()
            .filter(|l| start <= l.item && l.item < end)
            .collect();
        let mut scope = selected.label.clone().unwrap_or_default();
        if end > start && (start > 0 || end < selected.items.len()) {
            scope.push_str(&format!(", showing {}..{}", start + 1, end));
        }
        let position = Position {
            scope,
            first: shown.first().map(|l| l.hit.r#ref.to_string()),
            last: shown.last().map(|l| l.hit.r#ref.to_string()),
            shown: u32::try_from(end.saturating_sub(start)).unwrap_or(u32::MAX),
            total: u32::try_from(selected.items.len()).unwrap_or(u32::MAX),
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
