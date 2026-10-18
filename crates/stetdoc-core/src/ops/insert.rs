//! `insert` and `row`: new blocks, and the style lookup that names their paragraphs (design `document-and-ops.md` 2.3).

use super::context::{Context, Done};
use super::target;
use super::text::{follow_quotes, follow_quotes_in, quotes_to_follow};
use crate::api::error::{ErrorCode, StetError};
use crate::api::files::Place as ApiPlace;
use crate::api::op::{InsertOp, RowAction, RowOp};
use crate::refs::BaseToken;
use crate::revisions::compiler::Compiler;
use crate::revisions::types::{InsertRun, NewParagraph, Place, PropEdit};
use crate::state::Tx;
use crate::wordml::{BlockKey, BlockKind, StyleKind};

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

fn place_of(place: ApiPlace) -> Place {
    match place {
        ApiPlace::Before => Place::Before,
        ApiPlace::After => Place::After,
    }
}

/// How close a style name is to what was typed: names that hold it come first, then the shorter ones.
pub(super) fn nearest(typed: &str, names: &[String]) -> String {
    let typed = typed.to_lowercase();
    let mut ranked: Vec<&String> = names.iter().collect();
    ranked.sort_by_key(|n| {
        let lower = n.to_lowercase();
        (
            !(lower.contains(&typed) || typed.contains(&lower)),
            n.len().abs_diff(typed.len()),
        )
    });
    let shown: Vec<String> = ranked
        .into_iter()
        .take(6)
        .map(|n| format!("`{n}`"))
        .collect();
    shown.join(", ")
}

/// The `w:styleId` of a paragraph style, as `props::style_id` finds it.
pub(super) fn style_id(tx: &Tx<'_>, typed: &str) -> Result<String, StetError> {
    super::props::style_id(tx, StyleKind::Paragraph, typed)
}

/// `insert`: new paragraphs before or after a paragraph or a table, in order.
pub(super) fn insert(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &InsertOp,
) -> Result<Done, StetError> {
    let anchor = target::block(c.tx(), base, &op.r#ref)?;
    let (kind, _) = target::kind_of(c.tx(), &anchor)?;
    if kind == BlockKind::Row {
        return Err(invalid(format!(
            "{} is a table row. Insert a row with `row`, and paragraphs next to a paragraph or a table.",
            op.r#ref
        )));
    }
    let mut done = Done::naming(&[&anchor]);
    let mut at: BlockKey = anchor.clone();
    let mut place = place_of(op.place);
    // The quotes follow the anchor and its neighbours as they were before the first paragraph went in, read once.
    let mut quotes = op
        .paragraphs
        .iter()
        .any(|p| quotes_to_follow(cx, &p.text))
        .then(|| target::quote_context(c.tx(), &anchor));
    // Each paragraph after the first goes after the one before it, so the run carries its neighbours.
    let mut run = InsertRun::default();
    for paragraph in &op.paragraphs {
        let style = match &paragraph.style {
            Some(typed) => Some(style_id(c.tx(), typed)?),
            None => None,
        };
        // The quotes follow the paragraph the new one is next to, which is the anchor, whatever came before.
        let text = follow_quotes_in(
            c.tx(),
            cx,
            &anchor,
            ("", &paragraph.text, None),
            &mut done.warnings,
            &mut quotes,
        );
        let effect = c.insert_block_in(
            &mut run,
            &at,
            place,
            &NewParagraph {
                text,
                props: PropEdit::default(),
                style,
            },
        )?;
        let made = effect.new_blocks.first().cloned().ok_or_else(|| {
            StetError::new(ErrorCode::Internal, "internal: an insertion made no block")
        })?;
        done.effects.push(effect);
        // Each next paragraph goes after the one before it, as repeated Enters would put it.
        at = made;
        place = Place::After;
    }
    Ok(done)
}

/// `row`: a table row inserted next to a row, or deleted.
pub(super) fn row(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &RowOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    let (kind, _) = target::kind_of(c.tx(), &key)?;
    if kind != BlockKind::Row {
        return Err(invalid(format!(
            "{} is not a table row. `row` takes the ref of a row, such as r17.",
            op.r#ref
        )));
    }
    let mut done = Done::naming(&[&key]);
    match op.action {
        RowAction::Delete => {
            if op.cells.is_some() || op.place.is_some() {
                return Err(invalid(
                    "`cells` and `place` are for a row that is inserted.",
                ));
            }
            done.effects.push(c.delete_block(&key)?);
        }
        RowAction::Insert => {
            let mut cells = Vec::new();
            for text in op.cells.as_deref().unwrap_or_default() {
                cells.push(follow_quotes(
                    c.tx(),
                    cx,
                    &key,
                    ("", text, None),
                    &mut done.warnings,
                ));
            }
            let place = place_of(op.place.unwrap_or(ApiPlace::After));
            done.effects.push(c.insert_row(&key, place, &cells)?);
        }
    }
    Ok(done)
}
