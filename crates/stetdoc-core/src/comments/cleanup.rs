//! The cleanup after other operations: a comment whose reference run is gone is removed with it.
//!
//! The compiler hoists a range marker and never deletes one, so a text edit never leaves a marker without
//! its partner. But an untracked deletion, an accepted deletion and a rejected insertion take whole runs
//! away, and a `w:commentReference` is a run. Nothing in Word shows a comment that has none, so leaving it
//! in `comments.xml` would leave a stranded comment and its rows. This looks only at comments that had a
//! reference run when the batch began, and at the ones the batch added: a comment that was already
//! stranded in the input is not this batch's to remove.

use super::delete::remove_comments;
use super::{CommentAction, CommentEffect, CommentWarning};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::revisions::compiler::Compiler;
use crate::threads::Threads;
use std::collections::HashSet;

/// What the cleanup did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reconciled {
    /// The comments that were removed, each with the warning `removed_with_text`.
    pub(crate) removed: Vec<CommentEffect>,
    /// Comments that stay but whose range lost all its text (`anchor_emptied`).
    pub(crate) warnings: Vec<CommentWarning>,
}

/// Removes the comments of this transaction that lost their reference run, with their own markers and no
/// others, and says which comments were left with an empty range. It reads the base from the transaction,
/// and does nothing, quickly, in a document with no comment.
pub(crate) fn reconcile(c: &mut Compiler<'_, '_>) -> Result<Reconciled, StetError> {
    let (doomed, emptied) = {
        let tx = c.tx();
        let now_view = tx.parts();
        let now = Threads::of(&now_view)?;
        if now.comments().is_empty() {
            return Ok(Reconciled::default());
        }
        let base_view = tx.base().parts();
        let base = Threads::of(&base_view)?;
        let minted = tx.minted_annotation_ids();
        // The comments that had a reference run when the batch began, and the ones it made.
        let examined: HashSet<i64> = base
            .comments()
            .iter()
            .filter(|c| c.anchor.reference.is_some())
            .map(|c| c.id)
            .chain(
                now.comments()
                    .iter()
                    .map(|c| c.id)
                    .filter(|id| u32::try_from(*id).is_ok_and(|id| minted.contains(&id))),
            )
            .collect();
        let mut doomed = Vec::new();
        let mut emptied = Vec::new();
        for comment in now.comments().iter().filter(|c| examined.contains(&c.id)) {
            if now.is_ambiguous(comment.id) {
                continue;
            }
            if comment.anchor.reference.is_none() {
                doomed.push(comment.id);
                continue;
            }
            let covers = |t: &Threads, view: &crate::state::PartsView<'_>| {
                t.covered_text(view, comment.id, Mode::Final, 1)
            };
            if covers(&now, &now_view)?.is_some_and(|s| s.is_empty())
                && base.comment(comment.id).is_some()
                && covers(&base, &base_view)?.is_some_and(|s| !s.is_empty())
            {
                emptied.push(CommentWarning {
                    code: "anchor_emptied",
                    message: format!(
                        "The text that comment c{} was attached to was deleted. The comment stays, at the place where it was.",
                        comment.id
                    ),
                });
            }
        }
        (doomed, emptied)
    };
    let mut out = Reconciled {
        removed: Vec::new(),
        warnings: emptied,
    };
    if doomed.is_empty() {
        return Ok(out);
    }
    for g in remove_comments(c, &doomed)? {
        out.removed.push(CommentEffect {
            action: CommentAction::Removed,
            id: g.id,
            thread: g.thread,
            author: g.author,
            blocks: g.blocks,
            parts: g.parts,
            warnings: vec![CommentWarning {
                code: "removed_with_text",
                message: format!(
                    "Comment c{} was removed because the text it was attached to was removed.",
                    g.id
                ),
            }],
        });
    }
    Ok(out)
}
