//! A missing input path is `not_found`, with the message `view` gives, for every command that takes an input path. The
//! command must say so before it sniffs what the path was meant to be, so no command reports a missing file as the wrong
//! kind of file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use stetdoc_testkit::fixtures::contract;

const MISSING: &str = "nothere.stet";

fn stetdoc(cwd: &Path, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env("STETDOC_AUTHOR", "Clauseo")
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run stetdoc");
    child.stdin.take().unwrap().write_all(b"").unwrap();
    child.wait_with_output().unwrap()
}

/// The argument lists of one command per family, each naming `MISSING` as its input. `compare` gets a real second file,
/// so that the missing one is the only thing wrong with the call.
fn commands() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("view", vec!["view", MISSING]),
        ("inspect", vec!["inspect", MISSING, "p1"]),
        ("lookup", vec!["lookup", MISSING, "p1"]),
        ("check", vec!["check", MISSING]),
        ("unpack", vec!["unpack", MISSING]),
        ("pack", vec!["pack", MISSING]),
        (
            "edit",
            vec!["edit", MISSING, "--author", "Jane Doe", "--dry-run"],
        ),
        (
            "edit -o",
            vec!["edit", MISSING, "-o", "out.docx", "--author", "Jane Doe"],
        ),
        (
            "replace",
            vec!["replace", MISSING, "a", "b", "--author", "Jane Doe"],
        ),
        (
            "comment",
            vec!["comment", MISSING, "a", "b", "--author", "Jane Doe"],
        ),
        (
            "compare",
            vec![
                "compare",
                MISSING,
                "contract.docx",
                "-o",
                "out.docx",
                "--author",
                "Jane Doe",
            ],
        ),
        (
            "compare revised",
            vec![
                "compare",
                "contract.docx",
                MISSING,
                "-o",
                "out.docx",
                "--author",
                "Jane Doe",
            ],
        ),
    ]
}

#[test]
fn a_missing_input_is_not_found_with_the_message_view_gives_for_every_command() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    fs::write(cwd.join("contract.docx"), contract::package()).unwrap();
    let view = stetdoc(cwd, &["view", MISSING]);
    assert_eq!(view.status.code(), Some(3));
    let expected = String::from_utf8(view.stderr).unwrap();
    assert!(
        expected.starts_with("error[not_found]: There is no file at nothere.stet."),
        "{expected}"
    );
    for (name, args) in commands() {
        let out = stetdoc(cwd, &args);
        let stderr = String::from_utf8(out.stderr).unwrap();
        assert_eq!(out.status.code(), Some(3), "{name}: {stderr}");
        assert_eq!(stderr, expected, "{name} says what view says");
        assert!(out.stdout.is_empty(), "{name} prints nothing on stdout");
    }
    assert!(!cwd.join("out.docx").exists(), "nothing is written");
}
