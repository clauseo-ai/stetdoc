//! Constructors for the error codes the comment operations raise. Each message says what to do.

use crate::api::error::{ErrorCode, StetError};

/// A comment that does not exist.
pub(super) fn not_found(id: i64) -> StetError {
    StetError::new(
        ErrorCode::NotFound,
        format!("There is no comment c{id}. List the comments to see the ids."),
    )
}

/// Two comments have the id, so the operation cannot say which it means.
pub(super) fn ambiguous(id: i64) -> StetError {
    StetError::new(
        ErrorCode::AmbiguousMatch,
        format!("Two comments have the id c{id}, so the operation cannot tell which one it means."),
    )
}

/// The comment belongs to someone else.
pub(super) fn not_yours(id: i64, author: &str, who: &str) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "Comment c{id} is by {author}, and only its author ({who} is not) can change or delete it. Reply to it instead."
        ),
    )
}

/// The caller passed something that cannot be written.
pub(super) fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The request is well formed and this release does not do it.
pub(super) fn unsupported(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Unsupported, message)
}

/// A check of our own failed: a bug.
pub(super) fn internal(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Internal, message)
}

/// A note that does not exist.
pub(super) fn not_found_note(word: &str, id: i64) -> StetError {
    StetError::new(
        ErrorCode::NotFound,
        format!("There is no {word} with the id {id}. List the notes to see them."),
    )
}
