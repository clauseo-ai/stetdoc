//! The outline of the body, as a view shows it: the headings to a depth, paged like blocks. A view of the whole body
//! that does not fit one page shows the outline, with a warning that says how big the body is and how to read it.

use super::{Cx, OVERHEAD, Page};
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::{Cursor, Position, View, Warning, WarningCode};
use crate::projection::build::{Selection, project, story_warnings};
use crate::projection::model::{Body, Outline, OutlineEntry, Output};
use crate::projection::outline::{Entry, Full, headings};
use crate::projection::page::{estimate, fit, greedy};
use crate::projection::select::{Selected, Source};
use crate::state::PartId;
use crate::wordml::{BlockKind, StoryKind};

impl Cx<'_> {
    /// The warning of a view of the whole body that shows its outline: how big the body is, and how to read it.
    pub(super) fn outline_shown(&self, selected: &Selected) -> Result<Warning, StetError> {
        selected.load_sizes(&self.parts, false)?;
        let tokens = selected
            .items
            .iter()
            .map(|i| self.item_weight(selected, i))
            .fold(0u32, u32::saturating_add);
        let first = selected
            .items
            .first()
            .and_then(|i| self.item_refs(selected, i).0);
        let last = selected
            .items
            .last()
            .and_then(|i| self.item_refs(selected, i).1);
        let range = match (first, last) {
            (Some(a), Some(b)) => format!("--range {a}..{b}"),
            _ => "--range".to_string(),
        };
        let size = if tokens >= 1000 {
            format!("{}K", tokens / 1000)
        } else {
            tokens.to_string()
        };
        Ok(Warning {
            code: WarningCode::OutlineShown,
            block: None,
            story: Some("body".to_string()),
            message: format!(
                "the body is about {size} tokens, more than one page of {}, so this view shows its outline and not the body.",
                self.budget()
            ),
            next: Some(format!(
                "Add {range} to read the body a page at a time (each page says how to get the next), or --all for all of it at once."
            )),
        })
    }

    /// The outline of the body, to a depth, or to the deepest level that fits the budget, with the outline note in its head.
    pub(super) fn outline(&self, depth: Option<u32>) -> Result<View, StetError> {
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
                let (_, entry) =
                    crate::projection::select::locate(&self.parts, self.state, c.as_str())?;
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
        projection.head.warnings.extend(self.outline_note.clone());
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
