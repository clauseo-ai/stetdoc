//! Manual markup (`Markup::Manual`, `Markup::ManualColor`): a text change shown with direct run formatting and no revision,
//! for a reader who asked for a redline without Track Changes.
//!
//! The text operations call `text` and `delete_paragraph` here in place of the compiler's `replace_text` and `delete_block`.
//! With revisions they are those calls. With manual markup the compiler is untracked, and a change keeps the old text: the
//! difference of the old text and the new, by words as a tracked replace has it, becomes for each piece the new text put in
//! just after the old (`replace_text` at an empty range), underlined, and the old text struck through. The pieces go from the
//! last to the first, so that each keeps its offsets. A new paragraph is underlined whole, and a deleted paragraph is struck
//! through whole and stays, with its mark. `split`, `merge`, `row` and the deletion of a table change structure that no run
//! formatting can show, so they are refused.

use super::context::{Context, Marked};
use super::props::{element, w_ns};
use super::target;
use crate::api::error::{ErrorCode, StetError};
use crate::api::options::Markup;
use crate::revisions::compiler::Compiler;
use crate::revisions::diff::{self, Cleanup};
use crate::revisions::outcome::Effect;
use crate::revisions::types::{PropEdit, PropTarget};
use crate::wordml::{BlockKey, BlockKind};
use std::ops::Range;

/// The underline of new text. Single, as Word's own markup of an insertion: the reader of a manual redline reads it the same way.
const UNDERLINE: &str = "single";
/// The colour of deleted text in colour: red.
const DELETED_COLOR: &str = "FF0000";
/// The colour of new text in colour: blue.
const INSERTED_COLOR: &str = "0000FF";

/// The refusal of an operation that manual markup cannot show.
pub(super) fn refuse(what: &str) -> StetError {
    StetError::new(
        ErrorCode::InvalidPayload,
        format!(
            "{what} changes the structure of the document, which manual markup (strikethrough and underline) cannot show."
        ),
    )
    .with_next(
        "Leave it out of the manual edit: replace, delete and insert text instead, or make this change in a separate tracked edit.",
    )
}

/// The formatting of deleted text and of new text, in the namespace of the paragraph.
fn formats(
    c: &Compiler<'_, '_>,
    markup: Markup,
    key: &BlockKey,
) -> Result<(PropEdit, PropEdit), StetError> {
    let ns = w_ns(c.tx(), key)?;
    let mut deleted = vec![element(&ns, "strike", &[])];
    let mut inserted = vec![element(&ns, "u", &[("val", UNDERLINE.to_string())])];
    if markup == Markup::ManualColor {
        deleted.push(element(&ns, "color", &[("val", DELETED_COLOR.to_string())]));
        inserted.push(element(
            &ns,
            "color",
            &[("val", INSERTED_COLOR.to_string())],
        ));
    }
    let edit = |set| PropEdit {
        set,
        clear: Vec::new(),
    };
    Ok((edit(deleted), edit(inserted)))
}

/// The characters `range` of the paragraph's Final text become `with`: as the compiler's `replace_text` with revisions, and as
/// manual markup otherwise.
pub(super) fn text(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    key: &BlockKey,
    range: Range<usize>,
    with: &str,
    marked: &mut Vec<Marked>,
) -> Result<Vec<Effect>, StetError> {
    if !cx.markup.is_manual() {
        return Ok(vec![c.replace_text(key, range, with)?]);
    }
    let para = target::paragraph(c.tx(), key, "the paragraph")?;
    let old: String = para
        .text
        .chars()
        .skip(range.start)
        .take(range.len())
        .collect();
    let (deleted, inserted) = formats(c, cx.markup, key)?;
    let mut effects = Vec::new();
    let mut said = Vec::new();
    for piece in diff::edits(&old, with, Cleanup::Readable).iter().rev() {
        let start = range.start + piece.range.start;
        let end = range.start + piece.range.end;
        said.push(Marked {
            block: key.clone(),
            struck: old
                .chars()
                .skip(piece.range.start)
                .take(piece.range.len())
                .collect(),
            underlined: piece.with.clone(),
        });
        if !piece.with.is_empty() {
            effects.push(c.replace_text(key, end..end, &piece.with)?);
            let added = end..end + piece.with.chars().count();
            effects.push(c.set_props(&run(key, added), &inserted)?);
        }
        if start < end {
            effects.push(c.set_props(&run(key, start..end), &deleted)?);
        }
    }
    marked.extend(said.into_iter().rev());
    Ok(effects)
}

/// A whole block goes: as the compiler's `delete_block` with revisions; with manual markup a paragraph's text is struck through
/// and the paragraph stays, and a table is refused.
pub(super) fn delete_block(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    key: &BlockKey,
    name: &str,
    marked: &mut Vec<Marked>,
) -> Result<Vec<Effect>, StetError> {
    if !cx.markup.is_manual() {
        return Ok(vec![c.delete_block(key)?]);
    }
    if target::kind_of(c.tx(), key)?.0 != BlockKind::Paragraph {
        return Err(refuse(&format!("Deleting the table {name}")));
    }
    let para = target::paragraph(c.tx(), key, name)?;
    if para.len == 0 {
        return Ok(Vec::new());
    }
    let (deleted, _) = formats(c, cx.markup, key)?;
    let effect = c.set_props(&run(key, 0..para.len), &deleted)?;
    marked.push(Marked {
        block: key.clone(),
        struck: para.text,
        underlined: String::new(),
    });
    Ok(vec![effect])
}

/// Underlines a paragraph that the edit made, with manual markup: all of its text.
pub(super) fn underline_new(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    key: &BlockKey,
    marked: &mut Vec<Marked>,
) -> Result<Option<Effect>, StetError> {
    if !cx.markup.is_manual() {
        return Ok(None);
    }
    let para = target::paragraph(c.tx(), key, "the new paragraph")?;
    if para.len == 0 {
        return Ok(None);
    }
    let (_, inserted) = formats(c, cx.markup, key)?;
    let effect = c.set_props(&run(key, 0..para.len), &inserted)?;
    marked.push(Marked {
        block: key.clone(),
        struck: String::new(),
        underlined: para.text,
    });
    Ok(Some(effect))
}

fn run(key: &BlockKey, range: Range<usize>) -> PropTarget {
    PropTarget::Run {
        block: key.clone(),
        range,
    }
}
