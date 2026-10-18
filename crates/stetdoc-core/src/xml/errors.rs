//! Constructors for the error codes this module raises.

use crate::api::error::{ErrorCode, StetError};

/// The input is not well-formed XML, or breaks a rule for package parts.
pub(super) fn malformed(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::MalformedInput, message)
}

/// The input exceeds a limit.
pub(super) fn limit(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::LimitExceeded, message)
}

/// The tree cannot do what the caller asked for this document.
pub(super) fn unsupported(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::Unsupported, message)
}

/// The caller passed something invalid.
pub(super) fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// The node does not exist in this tree.
pub(super) fn not_found(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::NotFound, message)
}

/// The same error with the name of the part it concerns at the start of its message. A specific
/// hint is kept.
pub(super) fn with_part(e: StetError, part: &str) -> StetError {
    let hint = (e.next() != e.code().next_hint()).then(|| e.next().to_owned());
    let named = StetError::new(e.code(), format!("{part}: {}", e.message()));
    match hint {
        Some(h) => named.with_next(h),
        None => named,
    }
}
