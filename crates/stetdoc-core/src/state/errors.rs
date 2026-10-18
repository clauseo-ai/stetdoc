//! Constructors for the error codes this module raises.

use crate::api::error::{ErrorCode, StetError};

/// A caller broke a rule of this module, or two structures disagree: a bug in stetdoc, never in the
/// input. The message starts with `internal:` so that it cannot be mistaken for a validation error.
pub(super) fn internal(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Internal, format!("internal: {}", message.into()))
}

/// A ref or a batch that names a document or a history this state does not have.
pub(super) fn stale(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::StaleRef, message)
}

/// Readable files written for another state of the document.
pub(super) fn stale_files(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::StaleFiles, message)
}

/// The caller sent something that cannot be accepted.
pub(super) fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The working copy's saved state does not match the document it came with.
pub(super) fn corrupt(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::CorruptComplement, message)
}

/// The document or the history is past a limit.
pub(super) fn limit(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::LimitExceeded, message)
}
