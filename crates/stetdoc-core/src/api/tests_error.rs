//! Tests of the error table and of `StetError`: the codes, their exit classes and hints, and the JSON form.

use super::error::*;
use std::collections::HashSet;

#[test]
fn codes_have_unique_snake_case_names() {
    let names: HashSet<_> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    assert_eq!(names.len(), ErrorCode::ALL.len());
    for name in names {
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{name} is not snake_case"
        );
    }
}

#[test]
fn wire_names_round_trip() {
    for code in ErrorCode::ALL {
        assert_eq!(ErrorCode::from_wire(code.as_str()), Some(*code));
    }
    assert_eq!(ErrorCode::from_wire("nope"), None);
}

#[test]
fn every_code_has_documentation_and_a_surface_neutral_hint() {
    for code in ErrorCode::ALL {
        assert!(
            code.description().ends_with('.'),
            "{code} doc is not a sentence"
        );
        let hint = code.next_hint();
        assert!(hint.ends_with('.'), "{code} hint is not a sentence");
        assert!(!hint.contains('`'), "{code} hint names a command: {hint}");
    }
}

#[test]
fn exit_classes_are_numbered_one_to_seven_in_order() {
    let numbers: Vec<u8> = ExitClass::ALL.iter().map(|c| c.number()).collect();
    assert_eq!(numbers, [1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn exit_classes_match_the_architecture_table() {
    assert_eq!(
        ErrorCode::Unsupported.exit_class(),
        ExitClass::OperationFailed
    );
    assert_eq!(ErrorCode::InvalidPayload.exit_class(), ExitClass::Usage);
    assert_eq!(ErrorCode::AmbiguousMatch.exit_class(), ExitClass::Target);
    assert_eq!(ErrorCode::PendingEdits.exit_class(), ExitClass::Stale);
    assert_eq!(
        ErrorCode::ValidationFailed.exit_class(),
        ExitClass::Validation
    );
    assert_eq!(ErrorCode::UnsupportedVersion.exit_class(), ExitClass::Input);
    assert_eq!(ErrorCode::Internal.exit_class(), ExitClass::Internal);
}

#[test]
fn the_internal_hint_asks_for_a_report_and_never_for_another_edit() {
    let hint = ErrorCode::Internal.next_hint();
    assert!(hint.starts_with("Report"), "{hint}");
    assert!(hint.contains("Do not try a different edit"), "{hint}");
}

#[test]
fn an_error_carries_its_code_message_hint_and_operation_index() {
    let error = StetError::new(ErrorCode::StaleRef, "p42 was deleted").at_op(3);
    assert_eq!(error.code(), ErrorCode::StaleRef);
    assert_eq!(error.message(), "p42 was deleted");
    assert_eq!(error.op_index(), Some(3));
    assert_eq!(error.next(), ErrorCode::StaleRef.next_hint());
    assert_eq!(
        error.to_string(),
        "stale_ref: p42 was deleted (operation 3)"
    );
}

#[test]
fn a_specific_hint_overrides_the_default() {
    let error = StetError::new(ErrorCode::NotFound, "no p9").with_next("run `stetdoc view`");
    assert_eq!(error.next(), "run `stetdoc view`");
    assert_eq!(error.op_index(), None);
}

#[test]
fn a_limit_error_names_the_limit_and_its_value_in_the_json_form() {
    let error = StetError::new(ErrorCode::LimitExceeded, "too big").with_limit(
        "max_archive_bytes",
        600,
        512,
    );
    assert_eq!(error.limit_name(), Some("max_archive_bytes"));
    assert_eq!(error.limit_value(), Some(600));
    assert_eq!(error.limit_max(), Some(512));
    let json = serde_json::to_value(&error).unwrap();
    assert_eq!(json["limit"], "max_archive_bytes");
    assert_eq!(json["value"], 600);
    assert_eq!(json["max"], 512);
}

#[test]
fn an_error_without_a_limit_has_no_limit_fields() {
    let error = StetError::new(ErrorCode::LimitExceeded, "the package has too many parts");
    assert_eq!(error.limit_name(), None);
    let json = serde_json::to_value(&error).unwrap();
    assert!(
        json.get("limit").is_none() && json.get("value").is_none() && json.get("max").is_none()
    );
}
