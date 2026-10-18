//! The operation conformance suite, over the contract fixture.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use stetdoc_testkit::laws::conformance::{CASES, check_case, check_coverage, check_forgeries};

#[test]
fn no_property_operation_forges_or_erases_a_revision() {
    check_forgeries().unwrap();
}

#[test]
fn every_operation_has_a_case_or_is_listed_as_unbuilt() {
    check_coverage().unwrap();
}

#[test]
fn every_built_operation_conforms() {
    let mut failures = Vec::new();
    for case in CASES {
        if let Err(why) = check_case(case) {
            failures.push(why);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn edits_of_files_on_the_contract_keep_their_laws() {
    stetdoc_testkit::laws::files::smoke(&stetdoc_testkit::fixtures::contract::package()).unwrap();
}
