//! Building the neutral model from the parts of a document.
//!
//! `project` reads the stories that a `Selection` names through `PartsView` and returns a `Projection`:
//! the head (the base, the authors, the counts, the warnings) and the stories of blocks. It is a function of
//! the view and its arguments, with no cache of its own and no iteration over a hash map, so the output is
//! deterministic and a dry run's view of a `Tx` builds the same way (design 3.8).
//!
//! - `facts.rs`: what is the same on every page, computed once per document.
//! - `layout.rs`: the top-level blocks of a story, with the collapsed fields.
//! - `blocks.rs`: paragraphs, tables, rows, cells and section lines. `inlines.rs` and `fields.rs`: the text.
//! - `stories.rs`: headers, footers, notes and comments as units. `warnings.rs`: what the reader must know.
//! - `marks.rs`: the reads of single elements that `wordml` does not offer yet.

mod blocks;
mod cx;
mod facts;
mod fields;
mod inlines;
pub(super) mod layout;
mod marks;
pub(super) mod scan;
pub(super) mod stories;
mod warnings;

#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests_blocks;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_inlines;
#[cfg(test)]
mod tests_laws;
#[cfg(test)]
mod tests_select;

use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::projection::model::{Body, Head, Output, Projection, Summary};
use crate::refs::BaseToken;
use crate::state::{PartId, PartsView};
use cx::StoryCx;
use warnings::Warnings;

pub(crate) use layout::Top;
pub(crate) use stories::{Pick, StorySel, TopPick};

/// Which blocks of which stories a build takes, in the order the model lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    /// The stories, each with what is taken of it.
    pub stories: Vec<(PartId, StorySel)>,
}

impl Selection {
    /// Every story of the document, whole, in story order: the readable files.
    pub(crate) fn files(parts: &PartsView<'_>) -> Result<Selection, StetError> {
        let list = parts.story_specs()?;
        Ok(Selection {
            stories: list
                .0
                .iter()
                .filter_map(|s| parts.id(&s.part))
                .map(|id| (id, StorySel::Whole))
                .collect(),
        })
    }
}

/// The warnings that are about the whole of a story and not one block, for a page that builds none of its
/// blocks, such as the outline.
pub(crate) fn story_warnings(
    parts: PartsView<'_>,
    mode: Mode,
    part: PartId,
) -> Result<Vec<crate::api::view::Warning>, StetError> {
    let facts = parts.doc_facts()?;
    let mut cx = StoryCx::new(parts, mode, facts, part)?;
    cx.story_warnings();
    Ok(cx.warnings.take())
}

/// The model of the selected blocks, in `mode`, for `output`.
pub(crate) fn project(
    parts: PartsView<'_>,
    base: BaseToken,
    mode: Mode,
    output: Output,
    selection: &Selection,
) -> Result<Projection, StetError> {
    let facts = parts.doc_facts()?;
    let mut stories = Vec::new();
    let mut warnings = Warnings::default();
    let mut readonly = 0u32;
    for (part, sel) in &selection.stories {
        let mut cx = StoryCx::new(parts, mode, facts.clone(), *part)?;
        if let Some(story) = cx.story_model(sel)? {
            stories.push(story);
        }
        readonly += cx.readonly;
        warnings.extend(cx.warnings.take());
    }
    let summary = Summary {
        changes: facts.changes.clone(),
        comments: facts.counts.comments,
        resolved: if facts.counts.comments > 0 {
            let counts = crate::threads::Threads::of(&parts)?.counts();
            u32::try_from(counts.resolved_threads).unwrap_or(u32::MAX)
        } else {
            0
        },
        footnotes: facts.counts.footnotes,
        endnotes: facts.counts.endnotes,
        sections: facts.counts.sections,
        readonly,
    };
    Ok(Projection {
        head: Head {
            base,
            mode,
            output,
            common_style: facts.common_style.clone(),
            authors: facts.authors.clone(),
            summary,
            position: None,
            warnings: warnings.take(),
        },
        body: Body::Stories(stories),
        appendix: Vec::new(),
    })
}
