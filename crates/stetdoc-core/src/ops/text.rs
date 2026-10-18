//! The text operations: `replace`, `delete`, `split` and `merge` (design `document-and-ops.md` 2.3).
//!
//! Each resolves its refs and range, applies the quote policy to the text it writes, and makes calls on the compiler. Nothing
//! here touches the document by itself.

use super::context::{Context, Done};
use super::quotes;
use super::target::{self, Within, warning};
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::{DeleteOp, MergeOp, ReplaceOp, SplitOp};
use crate::api::options::Quotes;
use crate::api::view::{Mode, Warning, WarningCode};
use crate::refs::BaseToken;
use crate::revisions::compiler::Compiler;
use crate::state::Tx;
use crate::wordml::{BlockKey, BlockKind, MARK_DELETED};

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// Follows the quotes of the paragraph the text goes into, and says what it changed. `text` replaces `old`, which is empty for
/// a new paragraph, and only what differs from `old` is changed. `before` is the character `old` goes after in that paragraph,
/// or `None` when it starts one.
pub(super) fn follow_quotes(
    tx: &Tx<'_>,
    cx: &Context<'_>,
    key: &BlockKey,
    texts: (&str, &str, Option<char>),
    warnings: &mut Vec<Warning>,
) -> String {
    follow_quotes_in(tx, cx, key, texts, warnings, &mut None)
}

/// True if the quote policy can change `text`: it is on, and the text holds a quote.
pub(super) fn quotes_to_follow(cx: &Context<'_>, text: &str) -> bool {
    cx.quotes != Quotes::AsTyped
        && text.contains(['"', '\'', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}'])
}

/// As [`follow_quotes`], with the context of `key` read at most once into `context` and taken from there by the calls after
/// it. The paragraphs of one `insert` follow their anchor as it was before the first of them went in (`ops::insert` reads it
/// then): reading it again for each one would scan the story again for each one.
pub(super) fn follow_quotes_in(
    tx: &Tx<'_>,
    cx: &Context<'_>,
    key: &BlockKey,
    (old, text, before): (&str, &str, Option<char>),
    warnings: &mut Vec<Warning>,
    context: &mut Option<quotes::Context>,
) -> String {
    if !quotes_to_follow(cx, text) {
        return text.to_string();
    }
    let context = *context.get_or_insert_with(|| target::quote_context(tx, key));
    let followed = quotes::follow_edit(old, text, context, before);
    if followed.curled > 0 {
        warnings.push(warning(
            WarningCode::QuotesCurled,
            tx,
            Some(key),
            format!(
                "{} straight quote{} curled to match the paragraph.",
                followed.curled,
                if followed.curled == 1 {
                    " was"
                } else {
                    "s were"
                }
            ),
        ));
    }
    if followed.straightened > 0 {
        warnings.push(warning(
            WarningCode::QuotesStraightened,
            tx,
            Some(key),
            format!(
                "{} curly quote{} straightened to match the paragraph.",
                followed.straightened,
                if followed.straightened == 1 {
                    " was"
                } else {
                    "s were"
                }
            ),
        ));
    }
    followed.text
}

/// A `no_change` warning for an operation whose calls changed nothing.
fn no_change(tx: &Tx<'_>, key: &BlockKey, what: &str) -> Warning {
    warning(
        WarningCode::NoChange,
        tx,
        Some(key),
        format!("{what} changed nothing: the paragraph already reads that way."),
    )
}

/// `replace`: a range, or the whole paragraph, becomes `with`.
pub(super) fn replace(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &ReplaceOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    let para = target::paragraph(c.tx(), &key, &op.r#ref)?;
    let within = Within {
        find: op.find.as_deref(),
        occurrence: op.occurrence,
        start: op.start,
        end: op.end,
    };
    let found = target::range(c.tx(), &para, &op.r#ref, &within, cx.unit)?;
    let mut done = Done::naming(&[&key]);
    if found.loose {
        done.warnings.push(warning(
            WarningCode::FindMatchedLoosely,
            c.tx(),
            Some(&key),
            "`find` matched only after curly and straight quotes were treated as equal.",
        ));
    }
    let before = found
        .range
        .start
        .checked_sub(1)
        .and_then(|i| para.text.chars().nth(i));
    let old: String = para
        .text
        .chars()
        .skip(found.range.start)
        .take(found.range.len())
        .collect();
    let with = follow_quotes(
        c.tx(),
        cx,
        &key,
        (&old, &op.with, before),
        &mut done.warnings,
    );
    let effect = c.replace_text(&key, found.range, &with)?;
    if effect.is_empty() {
        done.warnings
            .push(no_change(c.tx(), &key, &format!("replace {}", op.r#ref)));
    }
    done.effects.push(effect);
    Ok(done)
}

/// `delete`: a whole block, a range, or the blocks from `ref` through `to`.
pub(super) fn delete(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &DeleteOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    if let Some(to) = &op.to {
        return delete_through(c, cx, base, op, &key, to);
    }
    let within = Within {
        find: op.find.as_deref(),
        occurrence: op.occurrence,
        start: op.start,
        end: op.end,
    };
    let (kind, _) = target::kind_of(c.tx(), &key)?;
    let mut done = Done::naming(&[&key]);
    let effect = match kind {
        BlockKind::Row => {
            return Err(invalid(format!(
                "{} is a table row. Delete it with `row`, action delete.",
                op.r#ref
            )));
        }
        BlockKind::Table => {
            if !within.is_whole() {
                return Err(StetError::new(
                    ErrorCode::CrossBlock,
                    format!(
                        "{} is a table. A text range names a paragraph in one of its cells.",
                        op.r#ref
                    ),
                ));
            }
            c.delete_block(&key)?
        }
        BlockKind::Paragraph if within.is_whole() => c.delete_block(&key)?,
        BlockKind::Paragraph => {
            let para = target::paragraph(c.tx(), &key, &op.r#ref)?;
            let found = target::range(c.tx(), &para, &op.r#ref, &within, cx.unit)?;
            if found.loose {
                done.warnings.push(warning(
                    WarningCode::FindMatchedLoosely,
                    c.tx(),
                    Some(&key),
                    "`find` matched only after curly and straight quotes were treated as equal.",
                ));
            }
            c.replace_text(&key, found.range, "")?
        }
    };
    if effect.is_empty() {
        done.warnings
            .push(no_change(c.tx(), &key, &format!("delete {}", op.r#ref)));
    }
    done.effects.push(effect);
    Ok(done)
}

/// The paragraphs from `first` through `last` in one container, as keys, or `cross_block`.
fn paragraphs_between(
    tx: &Tx<'_>,
    first: &BlockKey,
    last: &BlockKey,
    names: (&str, &str),
) -> Result<Vec<BlockKey>, StetError> {
    let cross = |m: String| StetError::new(ErrorCode::CrossBlock, m);
    if first.part() != last.part() {
        return Err(invalid(format!(
            "{} and {} are in different stories, so no range runs from one to the other.",
            names.0, names.1
        )));
    }
    let view = tx.parts();
    let part = view.id(first.part()).ok_or_else(|| {
        StetError::new(
            ErrorCode::Internal,
            "internal: a block's part is not in the document",
        )
    })?;
    let story = view.story(part)?;
    let (a, b) = (story.index_of(first.node()), story.index_of(last.node()));
    let (Some(a), Some(b)) = (a, b) else {
        return Err(StetError::new(
            ErrorCode::StaleRef,
            "a block of the range is not in the document any more",
        ));
    };
    if a >= b {
        return Err(invalid(format!(
            "{} comes after {}: a range runs from the first block to the last.",
            names.0, names.1
        )));
    }
    let entries = story.blocks();
    let parent = entries.get(a).and_then(|e| e.parent);
    let mut out = Vec::new();
    for i in a..=b {
        let Some(entry) = entries.get(i) else {
            continue;
        };
        if entry.kind != BlockKind::Paragraph || entry.parent != parent {
            return Err(cross(format!(
                "the range from {} through {} holds a table or a block of another container. A range may hold only paragraphs of one container.",
                names.0, names.1
            )));
        }
        out.push(story.key(entry));
    }
    Ok(out)
}

/// `delete` with `to`: the end of the first paragraph, the paragraphs between, and the start of the last.
fn delete_through(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &DeleteOp,
    first: &BlockKey,
    to: &str,
) -> Result<Done, StetError> {
    let last = target::block(c.tx(), base, to)?;
    if last == *first {
        return delete_in_one(c, cx, op, first);
    }
    // Two paragraphs: `start` is in the first and `end` in the last, so the end point comes after the start point whatever the
    // two offsets are, as long as the last paragraph comes after the first, which `paragraphs_between` checks.
    let blocks = paragraphs_between(c.tx(), first, &last, (&op.r#ref, to))?;
    let p1 = target::paragraph(c.tx(), first, &op.r#ref)?;
    let p2 = target::paragraph(c.tx(), &last, to)?;
    let start = match op.start {
        Some(v) => target::offset(&p1, &op.r#ref, v, cx.unit)?,
        None => 0,
    };
    let end = match op.end {
        Some(v) => target::offset(&p2, to, v, cx.unit)?,
        None => p2.len,
    };
    let mut done = Done::naming(&[first, &last]);
    let mut effects = Vec::new();
    // The first paragraph: whole when the range starts at 0, else its tail.
    if start == 0 {
        effects.push(c.delete_block(first)?);
    } else if start < p1.len {
        effects.push(c.replace_text(first, start..p1.len, "")?);
    }
    // The paragraphs between go whole.
    for key in blocks.iter().skip(1).take(blocks.len().saturating_sub(2)) {
        effects.push(c.delete_block(key)?);
        done.targets.push(key.clone());
    }
    // The last: whole when the range reaches its end, else its head.
    if end >= p2.len {
        effects.push(c.delete_block(&last)?);
    } else if end > 0 {
        effects.push(c.replace_text(&last, 0..end, "")?);
    }
    // What is left of the two ends is one paragraph.
    if start > 0 && end < p2.len {
        effects.push(c.merge_blocks(first, &last)?);
    }
    if effects.iter().all(|e| e.is_empty()) {
        done.warnings.push(no_change(
            c.tx(),
            first,
            &format!("delete {} through {}", op.r#ref, to),
        ));
    }
    done.effects = effects;
    Ok(done)
}

/// `delete` with `to` naming the paragraph `ref` names: the range from `start` to `end` of that paragraph, whole when it runs from
/// the start to the end, as the two ends of a range across paragraphs go whole. The end point may not come before the start point.
fn delete_in_one(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    op: &DeleteOp,
    key: &BlockKey,
) -> Result<Done, StetError> {
    let para = target::paragraph(c.tx(), key, &op.r#ref)?;
    let start = match op.start {
        Some(v) => target::offset(&para, &op.r#ref, v, cx.unit)?,
        None => 0,
    };
    let end = match op.end {
        Some(v) => target::offset(&para, &op.r#ref, v, cx.unit)?,
        None => para.len,
    };
    if end < start {
        return Err(invalid(format!(
            "`end` ({end}) is before `start` ({start}) in {}: a range ends after it starts.",
            op.r#ref
        )));
    }
    let mut done = Done::naming(&[key]);
    let effect = if start == 0 && end >= para.len {
        c.delete_block(key)?
    } else {
        c.replace_text(key, start..end, "")?
    };
    if effect.is_empty() {
        done.warnings
            .push(no_change(c.tx(), key, &format!("delete {}", op.r#ref)));
    }
    done.effects.push(effect);
    Ok(done)
}

/// `split`: a paragraph in two, at an offset or before or after some text.
pub(super) fn split(
    c: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &SplitOp,
) -> Result<Done, StetError> {
    let key = target::block(c.tx(), base, &op.r#ref)?;
    let para = target::paragraph(c.tx(), &key, &op.r#ref)?;
    let mut done = Done::naming(&[&key]);
    let at = match (op.at, &op.before, &op.after) {
        (Some(at), None, None) => target::offset(&para, &op.r#ref, at, cx.unit)?,
        (None, Some(text), None) | (None, None, Some(text)) => {
            let found = target::find(&para, &op.r#ref, text, None, c.tx())?;
            if found.loose {
                done.warnings.push(warning(
                    WarningCode::FindMatchedLoosely,
                    c.tx(),
                    Some(&key),
                    "the text matched only after curly and straight quotes were treated as equal.",
                ));
            }
            if op.before.is_some() {
                found.range.start
            } else {
                found.range.end
            }
        }
        _ => {
            return Err(invalid(
                "`split` takes exactly one of `at`, `before` and `after`.",
            ));
        }
    };
    done.effects.push(c.split_block(&key, at)?);
    Ok(done)
}

/// The paragraph after `key` in its container that is not a deleted block, if there is one.
fn next_paragraph(tx: &Tx<'_>, key: &BlockKey) -> Option<BlockKey> {
    let view = tx.parts();
    let part = view.id(key.part())?;
    let story = view.story(part).ok()?;
    let tree = view.tree(part).ok()?;
    let index = story.index_of(key.node())?;
    let entries = story.blocks();
    let parent = entries.get(index)?.parent;
    for (i, entry) in entries.iter().enumerate().skip(index + 1) {
        if entry.parent != parent || entry.kind != BlockKind::Paragraph {
            continue;
        }
        let empty = story
            .paragraph(tree, i)
            .is_some_and(|p| p.map().len(Mode::Final) == 0);
        if entry.has(MARK_DELETED) && empty {
            continue;
        }
        return Some(story.key(entry));
    }
    None
}

/// `merge`: a paragraph and the one after it.
pub(super) fn merge(
    c: &mut Compiler<'_, '_>,
    _cx: &Context<'_>,
    base: &BaseToken,
    op: &MergeOp,
) -> Result<Done, StetError> {
    let first = target::block(c.tx(), base, &op.r#ref)?;
    target::paragraph(c.tx(), &first, &op.r#ref)?;
    let second = match &op.second {
        Some(text) => target::block(c.tx(), base, text)?,
        None => next_paragraph(c.tx(), &first).ok_or_else(|| {
            StetError::new(
                ErrorCode::CrossBlock,
                format!(
                    "{} has no paragraph after it in its container to merge with.",
                    op.r#ref
                ),
            )
        })?,
    };
    let mut done = Done::naming(&[&first, &second]);
    done.effects.push(c.merge_blocks(&first, &second)?);
    Ok(done)
}
