//! What `document` calls to write something for a person or an agent: the readable files of a state, and
//! (in `receipt.rs` and `lookup.rs`) the blocks a receipt shows and the answer to a lookup (design 2).

use super::build::{Selection, project};
use super::formats::{Format, files_format};
use super::model::{Block, Body, Inline, Output, Projection};
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

/// The name and SHA-256 of each readable file.
pub(crate) type FileHashes = Vec<(String, [u8; 32])>;

/// The hashes of the readable files of a state as an earlier release rendered them, with every marker that `stet/1` gained
/// since 0.9.3 turned off: no `label` on a field (decision 0017) and no item number on a row (decision 0019). A working
/// copy that such a release wrote recorded these hashes, and its files parse here as they always did, so `document` takes
/// them as this release's (design 5.10). `None` when no marker of the state is turned off, since then the files render the
/// same either way.
pub(crate) fn hashes_as_earlier_release(
    state: &State,
    format: &FormatId,
) -> Result<Option<FileHashes>, StetError> {
    let format = files_format(format.as_str())?;
    let mut model = files_model(state.parts(), state.base_token())?;
    if !as_earlier_release(&mut model) {
        return Ok(None);
    }
    Ok(Some(
        format
            .render(&model)
            .files
            .iter()
            .map(|(name, text)| (name.clone(), crate::refs::sha256(text.as_bytes())))
            .collect(),
    ))
}

/// Turns off, in a model of the files, every marker that an earlier release did not render. True if one was on.
fn as_earlier_release(model: &mut Projection) -> bool {
    let Body::Stories(stories) = &mut model.body else {
        return false;
    };
    let mut changed = false;
    for story in stories {
        for unit in &mut story.units {
            changed |= blocks_as_earlier_release(&mut unit.blocks);
        }
    }
    changed
}

fn blocks_as_earlier_release(blocks: &mut [Block]) -> bool {
    let mut changed = false;
    for block in blocks {
        match block {
            Block::Paragraph(p) => changed |= inlines_as_earlier_release(&mut p.inlines),
            Block::Table(t) => {
                for row in &mut t.rows {
                    changed |= row.item.take().is_some();
                    for cell in &mut row.cells {
                        changed |= !cell.revs.is_empty();
                        cell.revs.clear();
                        changed |= blocks_as_earlier_release(&mut cell.blocks);
                    }
                }
            }
            _ => {}
        }
    }
    changed
}

fn inlines_as_earlier_release(inlines: &mut [Inline]) -> bool {
    let mut changed = false;
    for inline in inlines {
        match inline {
            Inline::Field(field) => changed |= std::mem::take(&mut field.label),
            Inline::Rev { inlines, .. }
            | Inline::Link { inlines, .. }
            | Inline::Control { inlines, .. }
            | Inline::Hidden { inlines, .. }
            | Inline::LooksHidden { inlines, .. } => changed |= inlines_as_earlier_release(inlines),
            Inline::Text(_) | Inline::Atom(_) | Inline::Mark(_) => {}
        }
    }
    changed
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
