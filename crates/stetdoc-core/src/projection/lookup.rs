//! `lookup` and `render_lookup`: a query answered from the index, and the answer written by a format (design 7.5).
//!
//! `lookup` is `index::lookup` with the one thing `index` may not know, the syntax of a ref: a query that is a
//! ref names a block, and the state resolves it. `index::lookup` returns data and no syntax, and a caller that
//! wants text asks `render_lookup`, so that no module but a format knows how a line is written. The answer is a snapshot of one state: it carries the base token it was
//! made at and every ref in it is a ref of that token, so the head prints that token and not the document's
//! present one. The text of a block hit is the block as the format writes it, which `index` cannot know.

use super::formats::by_id;
use super::model::{Body, Head, Output, Projection, Summary};
use super::receipt::render_blocks;
use crate::api::error::StetError;
use crate::api::files::BlockSpec;
use crate::api::lookup::{Hit, Lookup};
use crate::api::view::{FormatId, Mode};
use crate::index;
use crate::refs::{BaseToken, BlockRef, Claim};
use crate::state::State;

/// The answer to a query at the state: every hit, and never one chosen. A query that is a ref names a block, and
/// a ref that does not resolve is the state's error (`not_found`, `stale_ref`), not a lookup that found nothing.
pub(crate) fn lookup(state: &State, query: &str) -> Result<Lookup, StetError> {
    let trimmed = query.trim();
    let block = BlockRef::parse(trimmed)
        .ok()
        .map(|_| state.resolve(Claim::Implied, trimmed))
        .transpose()?;
    index::lookup(state.parts(), &state.base_token().to_string(), query, block)
}

/// The text of a lookup in a format.
pub(crate) fn render_lookup(
    state: &State,
    lookup: &Lookup,
    format: &FormatId,
) -> Result<String, StetError> {
    let format = by_id(format.as_str())?;
    let base = BaseToken::parse(&lookup.base)?;
    let spec = BlockSpec {
        format: FormatId::new(format.id()),
        context: 0,
    };
    let mut lookup = lookup.clone();
    for hit in &mut lookup.hits {
        if let Hit::Block { block, text, .. } = hit
            && let Ok(key) = state.resolve(Claim::Implied, block)
        {
            let shown = render_blocks(state.parts(), base, &[key], &spec)?;
            if let Some(first) = shown.blocks.into_iter().next() {
                *text = first.text;
            }
        }
    }
    let projection = Projection {
        head: Head {
            base,
            mode: Mode::Redline,
            output: Output::View {
                name: "lookup".into(),
            },
            common_style: None,
            authors: Vec::new(),
            summary: Summary::default(),
            position: None,
            warnings: Vec::new(),
        },
        body: Body::Lookup(lookup),
        appendix: Vec::new(),
    };
    Ok(format
        .render(&projection)
        .files
        .into_iter()
        .next()
        .map(|(_, text)| text)
        .unwrap_or_default())
}
