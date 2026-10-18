//! Reading what the writers need: a comment's element and paragraphs, and the thread it is in.

use super::errors::{ambiguous, internal, not_found};
use crate::api::error::StetError;
use crate::state::PartsView;
use crate::threads::{Comment, Threads};
use crate::wordml::{BlockKind, NodeId};
use std::sync::Arc;

/// A comment as the comments story has it.
pub(super) struct Where {
    /// The part: `comments.xml`.
    pub(super) part: Arc<str>,
    /// The `w:comment` element.
    pub(super) element: NodeId,
    /// Its paragraphs, in order.
    pub(super) paragraphs: Vec<NodeId>,
}

/// The element and the paragraphs of `comment`.
pub(super) fn locate(
    view: &PartsView<'_>,
    threads: &Threads,
    comment: &Comment,
) -> Result<Where, StetError> {
    let part = threads
        .parts()
        .comments
        .ok_or_else(|| internal("a comment has no comments part"))?;
    let name = view
        .name(part)
        .cloned()
        .ok_or_else(|| internal("the comments part has no name"))?;
    let story = view.story(part)?;
    let element = story
        .units()
        .get(comment.unit)
        .map(|u| u.node)
        .ok_or_else(|| internal("a comment has no unit"))?;
    let paragraphs = story
        .blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph && e.unit as usize == comment.unit)
        .map(|e| e.node)
        .collect();
    Ok(Where {
        part: name,
        element,
        paragraphs,
    })
}

/// The comment with this id, if exactly one has it.
pub(super) fn find(threads: &Threads, id: i64) -> Result<&Comment, StetError> {
    if threads.is_ambiguous(id) {
        return Err(ambiguous(id));
    }
    threads.comment(id).ok_or_else(|| not_found(id))
}
