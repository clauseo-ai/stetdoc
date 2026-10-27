//! Where a paragraph is, as a find hit or an occurrence names it: the clause it is, or whose own text it is, and
//! otherwise the nearest heading.
//!
//! The clause in force at a paragraph is the last clause before it, and that is often not the clause the paragraph
//! belongs to. A paragraph with no number after a list, such as "For the avoidance of doubt, ..." after item (g), belongs
//! to the clause the list is in, not to (g). A recital after the parties "(1)" and "(2)" belongs to no clause at all. So a
//! paragraph takes the label of the clause in force only when it is that clause or comes after a clause that holds text
//! (`11.1`, `Clause 4`). After a list item, whose label is in brackets (`(g)`, `(2)`, `a)`), it takes the label of the
//! clause the item sits in, and when there is none, no label. A heading (a paragraph with an outline level) that is not a
//! clause ends the clause before it: what comes after it is under that heading.
//!
//! The headings are read once, forward, as the paragraphs are asked about in document order, so a page of hits costs one
//! pass over the paragraphs before the last hit.

use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::index::{Clause, Index};
use crate::state::{PartId, PartsView, SiteHandle};
use crate::styles::Cascade;
use crate::wordml::{BlockKind, Story};
use std::sync::Arc;

/// How many characters of a heading a place shows.
const HEADING: usize = 50;

/// Where a paragraph is.
pub(crate) struct Place<'i> {
    /// The clause the paragraph is, or whose own text it is.
    pub clause: Option<&'i Clause>,
    /// The position of that clause in the index.
    pub at: Option<usize>,
    /// When it is in no clause: the entry of the nearest heading before it, or of itself if it is a heading.
    pub heading: Option<usize>,
}

/// Finds the place of paragraphs, asked about in document order within each story.
pub(crate) struct Placer<'a, 'i> {
    parts: PartsView<'a>,
    index: &'i Index,
    cascade: Arc<Cascade>,
    story: Option<(PartId, SiteHandle<'a>)>,
    /// The next entry to read for a heading.
    next: usize,
    /// The last heading before `next`.
    heading: Option<usize>,
}

impl<'a, 'i> Placer<'a, 'i> {
    /// A placer over the index of `parts`.
    pub(crate) fn new(parts: PartsView<'a>, index: &'i Index) -> Result<Self, StetError> {
        Ok(Placer {
            parts,
            index,
            cascade: parts.cascade()?,
            story: None,
            next: 0,
            heading: None,
        })
    }

    /// The place of the paragraph at `entry` of `story`, the story of `part`.
    pub(crate) fn place(
        &mut self,
        part: PartId,
        story: &Story,
        entry: usize,
    ) -> Result<Place<'i>, StetError> {
        let heading = self.heading_before(part, story, entry)?;
        let index = self.index;
        let none = Place {
            clause: None,
            at: None,
            heading,
        };
        let Some(mut at) = index.context_at(story.part(), entry) else {
            return Ok(none);
        };
        let Some(mut clause) = index.clause(at) else {
            return Ok(none);
        };
        let after_heading = |c: &Clause| heading.is_some_and(|h| h > c.entry);
        if after_heading(clause) {
            return Ok(none);
        }
        if clause.entry != entry {
            while is_item(&clause.label) {
                let Some(up) = index.ancestors(at).first().copied() else {
                    return Ok(none);
                };
                let Some(c) = index.clause(up) else {
                    return Ok(none);
                };
                if after_heading(c) {
                    return Ok(none);
                }
                (at, clause) = (up, c);
            }
        }
        Ok(Place {
            clause: Some(clause),
            at: Some(at),
            heading: None,
        })
    }

    /// The entry of the last paragraph with an outline level at or before `entry`, reading forward from the last one
    /// asked about.
    fn heading_before(
        &mut self,
        part: PartId,
        story: &Story,
        entry: usize,
    ) -> Result<Option<usize>, StetError> {
        if self.story.as_ref().is_none_or(|(p, _)| *p != part) {
            self.story = Some((part, self.parts.sites(part, Mode::Final)?));
            (self.next, self.heading) = (0, None);
        }
        if entry < self.next {
            (self.next, self.heading) = (0, None);
        }
        let Some((_, sites)) = self.story.as_mut() else {
            return Ok(None);
        };
        for (i, block) in story
            .blocks()
            .iter()
            .enumerate()
            .take(entry + 1)
            .skip(self.next)
        {
            if block.kind != BlockKind::Paragraph {
                continue;
            }
            let site = sites.site_for(block.node, None)?;
            if self.cascade.outline_level(&site).is_some() {
                self.heading = Some(i);
            }
        }
        self.next = self.next.max(entry + 1);
        Ok(self.heading)
    }
}

/// True if a clause label is that of a list item, in brackets: `(g)`, `(2)`, `(iv)`, `a)`. A list item holds its own
/// text only, so a paragraph after it belongs to the clause the list is in.
fn is_item(label: &str) -> bool {
    let label = label.trim();
    label.starts_with('(') || label.ends_with(')')
}

/// The text of the heading at `entry`, cut to about 50 characters, on one line.
pub(crate) fn heading_text(
    parts: &PartsView<'_>,
    part: PartId,
    story: &Story,
    entry: usize,
) -> Result<String, StetError> {
    let tree = parts.tree(part)?;
    let text = story
        .paragraph(tree, entry)
        .map(|p| p.text(Mode::Final))
        .unwrap_or_default();
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
        .filter(|w| !w.is_empty())
        .collect();
    let line = words.join(" ");
    if line.chars().count() <= HEADING {
        return Ok(line);
    }
    let cut: String = line.chars().take(HEADING - 1).collect();
    Ok(format!("{}…", cut.trim_end()))
}
