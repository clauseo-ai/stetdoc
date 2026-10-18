//! Comments, written: add, reply, resolve, edit, delete, the cleanup after other operations, and the
//! deletion of notes.
//!
//! **Owns.** Everything that changes a comment: the `w:comment` elements and their paragraphs in
//! `comments.xml`, the rows of `commentsExtended`, `commentsIds`, `commentsExtensible` and `people`, the
//! creation of `comments.xml` and `commentsExtended.xml` when a document has none, the two comment styles,
//! and the removal of comments whose text was removed. Also the deletion of a note, which shares the part
//! and style helpers. The range markers in the stories are never written here: they go through the
//! compiler's `place_markers`, `place_markers_beside` and `remove_markers_in_part`.
//!
//! **Interface.** `add::{add, reply}`, `resolve::set_resolved`, `edit::edit`, `delete::delete`, `cleanup::reconcile` and `delete_note`, each over a
//! `Compiler`, returning `CommentEffect`s (the action, the comment, the blocks and parts it changed, and
//! warnings).
//!
//! **Invariants.**
//! - Comment operations are never tracked, whatever the compiler's `Track` is.
//! - Only the author of a comment may edit or delete it (`readonly_region`). Anyone may reply and resolve.
//! - A comment's `w:id` comes from `Tx::next_annotation_id`, its paragraphs' `paraId`s and its durable id
//!   from `Tx::mint_id`, so ids are the highest plus one and the same on a dry run and the real edit.
//! - Every extension part that exists is kept consistent with `comments.xml`: one row per comment that has a
//!   `paraId`, none for a comment that is gone. Only `commentsExtended.xml` is created, when it is needed.
//! - A document that is Strict gets parts in the Strict namespace and the Strict relationship type.
//! - A comment whose reference run was removed by another operation is removed with its own markers, and no
//!   other (`reconcile`). A comment that was already stranded in the input is left alone.
//!
//! **Depends on.** `api`, `package`, `xml`, `schema`, `wordml`, `state`, `revisions`, `threads`.
//!
//! **Start reading.** `comments/add.rs`. Design: `docs/design/comments-and-notes.md`.

pub(crate) mod add;
mod body;
pub(crate) mod cleanup;
pub(crate) mod delete;
pub(crate) mod edit;
mod errors;
mod lookup;
pub(crate) mod notes;
mod parts;
pub(crate) mod resolve;
mod rows;
mod styles;
#[cfg(test)]
mod tests_add;
#[cfg(test)]
mod tests_change;
#[cfg(test)]
mod tests_cleanup;
#[cfg(test)]
mod tests_laws;
#[cfg(test)]
mod tests_notes;
#[cfg(test)]
mod tests_parts;
#[cfg(test)]
mod tests_scale;
#[cfg(test)]
mod tests_support;
mod xmlw;

use crate::wordml::BlockKey;
use std::sync::Arc;

/// What an operation did to a comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommentAction {
    /// A new comment.
    Added,
    /// A new reply.
    Replied,
    /// A thread was resolved.
    Resolved,
    /// A thread was reopened.
    Reopened,
    /// A comment's text was rewritten.
    Edited,
    /// A comment was deleted.
    Deleted,
    /// A comment was removed because the text it was attached to was removed.
    Removed,
}

/// Something worth telling the agent that is not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentWarning {
    /// The stable name: `already_resolved`, `reply_to_resolved`, `anchor_emptied`, `removed_with_text`.
    pub(crate) code: &'static str,
    /// A sentence that says what happened.
    pub(crate) message: String,
}

/// What one comment operation did, for the receipt and for the scope guard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CommentEffect {
    /// What was done.
    pub(crate) action: CommentAction,
    /// The `w:id` of the comment, which is `c3` to an agent.
    pub(crate) id: i64,
    /// The `w:id` of the root of its thread.
    pub(crate) thread: i64,
    /// The comment's author.
    pub(crate) author: String,
    /// The paragraphs the comment is anchored to, for the scope guard and the receipt.
    pub(crate) blocks: Vec<BlockKey>,
    /// The parts this operation wrote or added, for the part-level scope guard.
    pub(crate) parts: Vec<Arc<str>>,
    /// Warnings.
    pub(crate) warnings: Vec<CommentWarning>,
}
