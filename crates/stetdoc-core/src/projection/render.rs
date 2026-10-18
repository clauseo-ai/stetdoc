//! What `document` calls to write something for a person or an agent: the readable files of a state, and
//! (in `receipt.rs` and `lookup.rs`) the blocks a receipt shows and the answer to a lookup (design 2).

use super::build::{Selection, project};
use super::formats::{Format, files_format};
use super::model::{Block, Body, Output, Projection};
use crate::api::error::StetError;
use crate::api::files::ReadableFiles;
use crate::api::view::{FormatId, Mode};
use crate::refs::{BaseToken, BlockRef, Claim};
use crate::state::{PartsView, State};

/// The model of the readable files of a state: every story whole, in `Redline`, which is what the files
/// are and what a baseline is compared with.
pub(crate) fn files_model(parts: PartsView<'_>, base: BaseToken) -> Result<Projection, StetError> {
    let selection = Selection::files(&parts)?;
    project(parts, base, Mode::Redline, Output::Files, &selection)
}

/// The readable files of a state, written by the format: `document.md` and the files of the other stories
/// that the document has. `json/1` is never a working-copy format and is refused as `unsupported`.
pub(crate) fn render_files(state: &State, format: &FormatId) -> Result<ReadableFiles, StetError> {
    render_files_in(state, files_format(format.as_str())?)
}

/// The readable files of a state in any registered format, `json/1` included, which the alignment tests use so
/// that a bug in `stet/1`'s parser cannot hide one in `to_batch` (design 5.8).
pub(crate) fn render_files_in(
    state: &State,
    format: &dyn Format,
) -> Result<ReadableFiles, StetError> {
    let model = files_model(state.parts(), state.base_token())?;
    Ok(ReadableFiles {
        base: state.base_token().to_string(),
        format: FormatId::new(format.id()),
        files: format.render(&model).files,
        rendered: Vec::new(),
    })
}

/// Fills each paragraph's `final_text` with its text in `Final` as the document's text map gives it, the text
/// that the offsets of an operation count in, for a format that shows it (the JSON view, design 4.8). A
/// paragraph with no ref, or one the state no longer has, is left `None`.
pub(crate) fn fill_final_text(state: &State, projection: &mut Projection) {
    let parts = state.parts();
    let Body::Stories(stories) = &mut projection.body else {
        return;
    };
    for story in stories {
        for unit in &mut story.units {
            fill_blocks(state, &parts, &mut unit.blocks);
        }
    }
}

fn fill_blocks(state: &State, parts: &PartsView<'_>, blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => {
                p.final_text = p
                    .r#ref
                    .and_then(|r| final_text_of(state, parts, r))
                    .map(String::into_boxed_str);
            }
            Block::Table(t) => {
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        fill_blocks(state, parts, &mut cell.blocks);
                    }
                }
            }
            _ => {}
        }
    }
}

fn final_text_of(state: &State, parts: &PartsView<'_>, r: BlockRef) -> Option<String> {
    let key = state.resolve(Claim::Implied, &r.to_string()).ok()?;
    let part = parts.id(key.part())?;
    let story = parts.story(part).ok()?;
    let tree = parts.tree(part).ok()?;
    let entry = story.index_of(key.node())?;
    let p = story.paragraph(tree, entry)?;
    Some(p.map().text(Mode::Final).into_owned())
}
