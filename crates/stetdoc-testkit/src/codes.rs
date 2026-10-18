//! The one place the testkit looks at the core's error vocabulary.
//!
//! Everything else in `corpus`, `expect` and `fixtures` is plain data (strings, bytes, paths and
//! enums defined in this crate). The core's own tests may depend on this crate, and when they
//! do, the core is compiled twice: once as the library under test and once as this crate's
//! dependency. A core type that crossed the API would then be two different types. So the
//! codes are checked here, against the real `ErrorCode` wire names, and leave as `&'static str`.

use stetdoc_core::ErrorCode;

/// The codes stetdoc may refuse an input document with.
pub(crate) const REFUSALS: [&str; 3] = ["malformed_input", "limit_exceeded", "unsupported"];

/// The wire name of `name` when it is both an `ErrorCode` and a code a refusal may carry.
pub(crate) fn refusal(name: &str) -> Option<&'static str> {
    let wire = ErrorCode::ALL
        .iter()
        .map(|c| c.as_str())
        .find(|wire| *wire == name)?;
    REFUSALS.contains(&wire).then_some(wire)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_refusal_code_is_an_error_code_wire_name() {
        for code in REFUSALS {
            assert_eq!(refusal(code), Some(code), "{code}");
        }
    }

    #[test]
    fn other_codes_and_other_words_are_not_refusals() {
        assert_eq!(refusal("stale_ref"), None);
        assert_eq!(refusal("ok"), None);
        assert_eq!(refusal("MalformedInput"), None);
    }
}
