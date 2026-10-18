//! Resolving a thread and reopening it.
//!
//! Word reads a thread as resolved when its root's `w15:done` is set, and writes it on every comment of
//! the thread (the `word-authored-threaded-comments` document has both a parent and its reply at `1`), so
//! that is what is written. No story block changes, so none is touched: the scope guard reads the
//! anchor blocks from the effect.

use super::add::give_para_id_at;
use super::errors::internal;
use super::lookup::{find, locate};
use super::parts::{ensure_comments, ensure_extended};
use super::{CommentAction, CommentEffect, CommentWarning, rows};
use crate::api::error::StetError;
use crate::revisions::compiler::Compiler;
use crate::threads::Threads;
use crate::wordml::NodeId;
use std::sync::Arc;

/// Sets the thread that holds `comment` to resolved, or to open when `resolved` is false. Every comment of
/// the thread gets the same `done`. A comment with no `paraId` is given one, since a row needs it, and
/// `commentsExtended.xml` is made if the document has none. A thread that is already as asked is left
/// alone and the effect says so.
pub(crate) fn set_resolved(
    c: &mut Compiler<'_, '_>,
    comment: i64,
    resolved: bool,
) -> Result<CommentEffect, StetError> {
    let (root_id, author, blocks, members, already, missing) = {
        let view = c.tx().parts();
        let threads = Threads::of(&view)?;
        let asked = find(&threads, comment)?;
        let thread = threads
            .thread_of(asked.id)
            .ok_or_else(|| internal("a comment is in no thread"))?;
        let root = find(&threads, thread.root)?;
        let mut members = vec![root.id];
        for id in &thread.replies {
            find(&threads, *id)?;
            members.push(*id);
        }
        let already = members
            .iter()
            .all(|id| threads.comment(*id).is_some_and(|c| c.resolved == resolved));
        // Each member's \`paraId\` is read here, once. A comment that has none is found in the comments part,
        // to be given one.
        let mut wanted: Vec<(i64, Option<u32>)> = Vec::new();
        let mut missing: Vec<(usize, i64, Arc<str>, NodeId)> = Vec::new();
        for id in &members {
            let c = find(&threads, *id)?;
            if c.para_id.is_none() && !already {
                let at = locate(&view, &threads, c)?;
                let last = *at
                    .paragraphs
                    .last()
                    .ok_or_else(|| internal("a comment has no paragraph"))?;
                missing.push((wanted.len(), c.id, at.part, last));
            }
            wanted.push((c.id, c.para_id));
        }
        (
            root.id,
            root.author.clone().unwrap_or_default(),
            root.anchor.blocks.clone(),
            wanted,
            already,
            missing,
        )
    };
    let action = if resolved {
        CommentAction::Resolved
    } else {
        CommentAction::Reopened
    };
    let mut effect = CommentEffect {
        action,
        id: comment,
        thread: root_id,
        author,
        blocks,
        parts: Vec::new(),
        warnings: Vec::new(),
    };
    if already {
        effect.warnings.push(CommentWarning {
            code: if resolved {
                "already_resolved"
            } else {
                "already_open"
            },
            message: format!(
                "Thread c{root_id} is already {}. Nothing changed.",
                if resolved { "resolved" } else { "open" }
            ),
        });
        return Ok(effect);
    }
    let mut writer = c.parts_mut();
    let tx = &mut writer;
    let mut parts: Vec<Arc<str>> = Vec::new();
    let comments = ensure_comments(tx)?;
    let extended = ensure_extended(tx)?;
    let mut paras: Vec<Option<u32>> = members.iter().map(|(_, p)| *p).collect();
    for (slot, _, part, node) in missing {
        let p = give_para_id_at(tx, &part, node)?;
        if let Some(s) = paras.get_mut(slot) {
            *s = Some(p);
        }
        parts.push(Arc::clone(&comments));
    }
    let wanted: Vec<(u32, bool)> = paras.into_iter().flatten().map(|p| (p, resolved)).collect();
    if rows::set_done_many(tx, &extended, &wanted)? {
        parts.push(Arc::clone(&extended));
    }
    parts.dedup();
    effect.parts = parts;
    Ok(effect)
}
