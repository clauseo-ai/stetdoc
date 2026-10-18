//! Constructors for the error codes this module raises.

use crate::api::error::{ErrorCode, StetError};

/// A ref names a block that is gone, or the batch was written for another state.
pub(super) fn stale(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::StaleRef, message)
}

/// A ref that never existed.
pub(super) fn not_found(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::NotFound, message)
}

/// The table or the history would grow past `Limits::max_refs`.
pub(super) fn limit(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::LimitExceeded, message)
}

/// A caller contradicted the table: a bug in stetdoc, never the input.
pub(super) fn internal(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Internal, format!("internal: {}", message.into()))
}

/// The persisted ref map is damaged or does not match the package.
pub(super) fn corrupt(message: impl Into<String>) -> StetError {
    StetError::new(
        ErrorCode::CorruptComplement,
        format!("the ref map is damaged: {}", message.into()),
    )
}

/// The persisted ref map was written by a release this one cannot read.
pub(super) fn unsupported_version(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::UnsupportedVersion, message)
}
