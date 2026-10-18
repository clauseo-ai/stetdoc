//! Constructors for the error codes this module raises.

use crate::api::error::{ErrorCode, StetError};

/// The input is not a valid package.
pub(super) fn malformed(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::MalformedInput, message)
}

/// The input exceeds a limit.
pub(super) fn limit(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::LimitExceeded, message)
}

/// The package uses something this version does not read or write.
pub(super) fn unsupported(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Unsupported, message)
}

/// The caller passed something invalid.
pub(super) fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The named part does not exist.
pub(super) fn not_found(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::NotFound, message)
}
