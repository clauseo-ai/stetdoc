//! Operations into compiler calls.
//!
//! **Owns.** One file per family of operations (resolve the refs and the range, apply the quote policy, call the compiler),
//! dispatch, the scope and its guard, and the batch parsers (JSON and block text), which are generated from the operation
//! specs.
//!
//! **Interface.** `apply(&Batch, &mut Tx, &Context) -> Result<Applied, StetError>`, `Batch::{parse, to_json, to_block}`
//! (`parse/`), `validate::batch`, `scope::{ScopeSet, guard, guard_changed}` and `planned::batch`.
//!
//! **Invariants.**
//! - A batch applies completely or not at all: the first failure ends it, and `document` drops the transaction.
//! - Dispatch is an exhaustive match over `Op`, so the compiler points to every place a new operation must be handled.
//! - Every operation compiles through `revisions::Compiler`, and `ops` adds nothing of its own to the document.
//! - The two syntaxes of a batch end in the same `Batch`, and every field of every operation is described once, in
//!   `api::op_spec`. A field that the table does not list is refused in JSON and in block text.
//! - Every error raised here carries the index of the operation that raised it. For an edit of files `document` turns the index
//!   into the file and the line of `Context::origins`, and the messages of the scope guard name that line (`OpApplied::by`).
//! - An operation sees the document as the operation before it left it: a ref, a `find` and an offset are read from the
//!   staged text.
//!
//! **Depends on.** `api`, `schema`, `wordml`, `styles`, `refs`, `state`, `revisions`, `comments`.
//!
//! `schema` gives the names that a property container allows, which `props_names` checks.
//!
//! **Start reading.** `ops/mod.rs`, then any `ops/<name>.rs`.

mod comments;
mod context;
mod insert;
mod parse;
mod planned;
mod props;
mod props_names;
mod props_typed;
mod quotes;
mod review;
mod scope;
mod scope_frame;
mod target;
mod text;
mod units;
mod validate;

use crate::api::batch::Batch;
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::Op;
use crate::api::view::{Warning, WarningCode};
use crate::refs::BaseToken;
use crate::revisions::compiler::Compiler;
use crate::revisions::index::ChangeIndex;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::Track;
use crate::revisions::types::Action;
use crate::state::Tx;
use context::{Done, OpApplied};

pub(crate) use comments::comment_warning;
pub(crate) use context::{Applied, Context};
pub(crate) use planned::batch as planned_batch;
pub(crate) use scope::{ScopeSet, guard as scope_guard, guard_changed as scope_guard_changed};
pub(crate) use validate::batch as validate_batch;

/// Sets the index of the operation on an error that has none: a lower layer that set one knows better.
fn at(error: StetError, index: usize) -> StetError {
    if error.op_index().is_some() {
        error
    } else {
        error.at_op(index)
    }
}

fn is_review(op: &Op) -> bool {
    matches!(op, Op::Accept(_) | Op::Reject(_))
}

/// Who is said to change a block, in the sentence that refuses a change outside the scope. An edit of files names the file and
/// the line, which is what the agent wrote, and a batch names the operation.
fn by(cx: &Context<'_>, index: usize, op: &Op) -> String {
    match cx.origins.get(index) {
        Some(o) if o.line > 0 => {
            format!("The edit at {} line {} would change it.", o.file, o.line)
        }
        Some(o) => format!(
            "The removal of {} from {} would change it.",
            o.block.as_deref().unwrap_or("a block"),
            o.file
        ),
        None => format!("Operation {index} ({}) would change it.", label(op)),
    }
}

/// What an operation is about, for a message: `replace p57`.
fn label(op: &Op) -> String {
    let named = match op {
        Op::Replace(o) => o.r#ref.clone(),
        Op::Insert(o) => o.r#ref.clone(),
        Op::Delete(o) => o.r#ref.clone(),
        Op::Split(o) => o.r#ref.clone(),
        Op::Merge(o) => o.r#ref.clone(),
        Op::Format(o) => o.r#ref.clone(),
        Op::Paragraph(o) => o.r#ref.clone(),
        Op::Style(o) => o.r#ref.clone(),
        Op::List(o) => o.r#ref.clone(),
        Op::Set(o) => o.r#ref.clone().unwrap_or_default(),
        Op::Row(o) => o.r#ref.clone(),
        Op::Comment(o) => o.r#ref.clone(),
        Op::Reply(o) => o.comment.clone(),
        Op::Resolve(o) => o.comment.clone(),
        Op::EditComment(o) => o.comment.clone(),
        Op::DeleteComment(o) => o.comment.clone(),
        Op::DeleteNote(o) => o.note.clone(),
        Op::Accept(o) | Op::Reject(o) => match (&o.all, &o.author, &o.ids) {
            (Some(_), ..) => "all".to_string(),
            (_, Some(author), _) => author.clone(),
            (_, _, Some(ids)) => ids
                .iter()
                .map(|i| format!("#{i}"))
                .collect::<Vec<_>>()
                .join(" "),
            _ => String::new(),
        },
    };
    if named.is_empty() {
        op.name().to_string()
    } else {
        format!("{} {named}", op.name())
    }
}

fn dispatch(
    compiler: &mut Compiler<'_, '_>,
    cx: &Context<'_>,
    base: &BaseToken,
    op: &Op,
) -> Result<Done, StetError> {
    match op {
        Op::Replace(o) => text::replace(compiler, cx, base, o),
        Op::Insert(o) => insert::insert(compiler, cx, base, o),
        Op::Delete(o) => text::delete(compiler, cx, base, o),
        Op::Split(o) => text::split(compiler, cx, base, o),
        Op::Merge(o) => text::merge(compiler, cx, base, o),
        Op::Row(o) => insert::row(compiler, cx, base, o),
        Op::Format(o) => props::format(compiler, cx, base, o),
        Op::Paragraph(o) => props::paragraph(compiler, base, o),
        Op::Style(o) => props::style(compiler, base, o),
        Op::List(o) => props::list(compiler, base, o),
        Op::Set(o) => props::set(compiler, cx, base, o),
        Op::Comment(o) => comments::comment(compiler, cx, base, o),
        Op::Reply(o) => comments::reply(compiler, cx, o),
        Op::Resolve(o) => comments::resolve(compiler, o),
        Op::EditComment(o) => comments::edit_comment(compiler, cx, o),
        Op::DeleteComment(o) => comments::delete_comment(compiler, cx, o),
        Op::DeleteNote(o) => comments::delete_note(compiler, cx, o),
        Op::Accept(_) | Op::Reject(_) => Err(StetError::new(
            ErrorCode::Internal,
            "internal: a review operation reached the compiler loop.",
        )),
    }
}

/// A warning for text that the compiler said in words.
fn compiler_warning(message: &str) -> Warning {
    Warning {
        code: WarningCode::NoChange,
        block: None,
        story: None,
        message: message.to_string(),
        next: None,
    }
}

/// A run of consecutive `accept` and `reject`: one call of `resolve`.
fn run_review(
    tx: &mut Tx<'_>,
    index: &mut ChangeIndex,
    cx: &Context<'_>,
    ops: &[Op],
    first: usize,
    applied: &mut Applied,
) -> Result<(), StetError> {
    let mut steps = Vec::new();
    for (k, op) in ops.iter().enumerate() {
        let (review, action) = match op {
            Op::Accept(r) => (r, Action::Accept),
            Op::Reject(r) => (r, Action::Reject),
            _ => continue,
        };
        steps.push(review::step(tx, cx, review, action).map_err(|e| at(e, first + k))?);
    }
    let resolutions = resolve(tx, index, &steps).map_err(|e| at(e, first))?;
    for (k, (op, resolution)) in ops.iter().zip(resolutions).enumerate() {
        applied.origins.extend(resolution.origins());
        let mut warnings: Vec<Warning> = resolution
            .warnings
            .iter()
            .map(|m| Warning {
                code: WarningCode::ReadAnomaly,
                block: None,
                story: None,
                message: m.clone(),
                next: None,
            })
            .collect();
        let changed = !resolution.resolved.is_empty();
        if !changed {
            warnings.push(Warning {
                code: WarningCode::NoChange,
                block: None,
                story: None,
                message: format!("{} resolved nothing: no revision matched.", label(op)),
                next: None,
            });
        }
        applied.ops.push(OpApplied {
            name: op.name(),
            by: by(cx, first + k, op),
            changed,
            warnings,
            resolution: Some(applied.resolutions.len()),
            ..OpApplied::default()
        });
        applied.resolutions.push((first + k, resolution));
    }
    Ok(())
}

/// Applies a batch to a transaction. The first failure ends it, with the index of the operation. Nothing is committed here.
///
/// One compiler serves the operations that write, and a run of `accept` and `reject` ends it, resolves, and starts a new one,
/// because both need the transaction.
pub(crate) fn apply(
    batch: &Batch,
    tx: &mut Tx<'_>,
    cx: &Context<'_>,
) -> Result<Applied, StetError> {
    let base = BaseToken::parse(&batch.base)?;
    // Untracked edits are still the stamp's author's: their own pending changes are edited in place, and another author's are
    // refused (`revisions/foreign.rs`).
    let track = if cx.tracked {
        Track::Tracked(cx.stamp.clone())
    } else {
        Track::UntrackedBy(cx.stamp.author().to_string())
    };
    let mut applied = Applied::default();
    let mut index = ChangeIndex::new();
    let ops = &batch.ops;
    let mut i = 0usize;
    while let Some(op) = ops.get(i) {
        let review = is_review(op);
        let end = ops
            .iter()
            .skip(i)
            .position(|o| is_review(o) != review)
            .map_or(ops.len(), |n| i + n);
        let run = ops.get(i..end).unwrap_or_default();
        if review {
            run_review(tx, &mut index, cx, run, i, &mut applied)?;
        } else {
            let mut compiler = Compiler::new(&mut *tx, track.clone()).map_err(|e| at(e, i))?;
            for (k, op) in run.iter().enumerate() {
                let n = i + k;
                let done = dispatch(&mut compiler, cx, &base, op).map_err(|e| at(e, n))?;
                // The index of revisions is built at the first `resolve`, and kept current by what the calls did before it.
                if ops.iter().skip(n + 1).any(is_review) {
                    for effect in &done.effects {
                        index.note(compiler.tx(), effect);
                    }
                }
                let changed = done.changed || done.effects.iter().any(|e| !e.is_empty());
                applied.side_effects.extend(done.side_effects);
                applied
                    .comments
                    .extend(done.comments.into_iter().map(|e| (Some(n), e)));
                applied.ops.push(OpApplied {
                    name: op.name(),
                    by: by(cx, n, op),
                    effects: done.effects,
                    targets: done.targets,
                    owned: done.owned,
                    changed,
                    warnings: done.warnings,
                    resolution: None,
                });
            }
            let outcome = compiler
                .finish()
                .map_err(|e| at(e, end.saturating_sub(1)))?;
            applied.origins.extend(outcome.origins());
            for message in &outcome.warnings {
                applied.warnings.push((None, compiler_warning(message)));
            }
            applied.outcomes.push(outcome);
        }
        i = end;
    }
    applied.touched_by_ops = tx.touched().len();
    reconcile(tx, &track, &mut applied)?;
    Ok(applied)
}

/// The cleanup after the operations, when the document has comments: a comment whose reference run an operation removed goes,
/// with its own markers. What it touches comes after `touched_by_ops`, and the scope guard does not hold it to the scope, because
/// the edit that removed the text was itself checked.
fn reconcile(tx: &mut Tx<'_>, track: &Track, applied: &mut Applied) -> Result<(), StetError> {
    if comments::comment_parts(tx).is_empty() {
        return Ok(());
    }
    let mut compiler = Compiler::new(&mut *tx, track.clone())?;
    let reconciled = crate::comments::cleanup::reconcile(&mut compiler)?;
    let outcome = compiler.finish()?;
    applied.origins.extend(outcome.origins());
    applied.outcomes.push(outcome);
    for w in &reconciled.warnings {
        applied.warnings.push((None, comments::comment_warning(w)));
    }
    for effect in reconciled.removed {
        for part in &effect.parts {
            applied.side_effects.push(context::SideEffect {
                part: part.to_string(),
                why: format!("the comment c{} was removed with its text", effect.id),
                styles: None,
            });
        }
        let blocks: Vec<String> = effect
            .blocks
            .iter()
            .filter_map(|k| tx.parts().ref_of(k).map(|r| r.to_string()))
            .collect();
        // The warning names the blocks whose markers went, which no operation named.
        for w in &effect.warnings {
            let mut shown = comments::comment_warning(w);
            if shown.code == WarningCode::CommentRemoved && !blocks.is_empty() {
                shown.message = format!(
                    "{} Its markers were in {}.",
                    shown.message.trim_end(),
                    blocks.join(", ")
                );
            }
            applied.warnings.push((None, shown));
        }
        applied.comments.push((None, effect));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
