//! Integration tests for the `stetdoc` binary.
//!
//! The allow below is the template for integration tests: clippy exempts `#[test]`
//! functions but not the helpers around them, and a panic in a test is a failure.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

fn stetdoc() -> Command {
    Command::new(env!("CARGO_BIN_EXE_stetdoc"))
}

#[test]
fn version_prints_the_package_version() {
    let output = stetdoc().arg("--version").output().expect("run stetdoc");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert_eq!(
        stdout.trim(),
        format!("stetdoc {}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn anything_else_is_a_usage_error_on_stderr() {
    let output = stetdoc().arg("view").output().expect("run stetdoc");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
}
