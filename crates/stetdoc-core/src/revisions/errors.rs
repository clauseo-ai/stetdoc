//! Constructors for the error codes this module raises. Each message says what to do, as
//! `ARCHITECTURE.md` requires, and the hint of the code is the default for the rest.

use crate::api::error::{ErrorCode, StetError};

/// The edit would change read-only content: a deleted block, a field, a locked control, or a change
/// that another author has pending.
pub(super) fn readonly(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::ReadonlyRegion, message)
}

/// The caller passed something that cannot be written.
pub(super) fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The edit spans blocks that a primitive does not span, or names a block of the wrong kind.
pub(super) fn cross_block(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::CrossBlock, message)
}

/// The request is well formed and this release does not do it.
pub(super) fn unsupported(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Unsupported, message)
}

/// A check of the compiler's own failed. It is a bug, and the batch is discarded.
pub(super) fn internal(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Internal, message)
}

/// The block is not in the document any more.
pub(super) fn stale(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::StaleRef, message)
}

/// A limit was reached.
pub(super) fn limit(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::LimitExceeded, message)
}

/// Nothing matches.
pub(super) fn not_found(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::NotFound, message)
}
