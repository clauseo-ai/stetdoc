//! The editor through its interface, on the contract fixture: view, str_replace, insert, and the failures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use stetdoc_core::{Document, ErrorCode, OpenOptions, RevisionFilter, Stamp};
use stetdoc_editor::{Command, Failure, Output, respond, tool_result};

/// A command with the files at the root.
fn run(doc: &mut Document, command: &Command, stamp: &Stamp) -> Result<Output, Failure> {
    stetdoc_editor::run(doc, command, stamp, "")
}

fn doc() -> Document {
    Document::open(
        stetdoc_testkit::fixtures::contract::package(),
        &OpenOptions::default(),
    )
    .unwrap()
}

fn stamp() -> Stamp {
    Stamp::new("Clauseo", "2026-10-06T10:00:00Z")
}

fn command(json: &str) -> Command {
    Command::parse(json).unwrap()
}

fn line_of(doc: &Document, needle: &str) -> usize {
    let files = doc.files().unwrap();
    files.files[0]
        .1
        .lines()
        .position(|line| line.contains(needle))
        .unwrap()
        + 1
}

#[test]
fn view_lists_the_files_and_shows_one_with_line_numbers() {
    let mut doc = doc();
    let list = run(
        &mut doc,
        &command(r#"{"command":"view","path":"/"}"#),
        &stamp(),
    )
    .unwrap();
    assert!(list.text.contains("document.md ("), "{}", list.text);
    let file = run(
        &mut doc,
        &command(r#"{"command":"view","path":"/document.md"}"#),
        &stamp(),
    )
    .unwrap();
    assert!(
        file.text.starts_with("     1\t{stet 1 file=document.md"),
        "{}",
        file.text
    );
    let n = line_of(&doc, "p4 Payment");
    assert!(
        file.text
            .contains(&format!("{n:>6}\tp4 Payment is due within thirty days.\n")),
        "{}",
        file.text
    );
    assert!(file.receipt.is_none());
}

#[test]
fn view_range_shows_those_lines_and_minus_one_is_the_end() {
    let mut doc = doc();
    let n = line_of(&doc, "p4 Payment");
    let input = format!(r#"{{"command":"view","path":"document.md","view_range":[{n},{n}]}}"#);
    let out = run(&mut doc, &command(&input), &stamp()).unwrap();
    assert_eq!(
        out.text,
        format!("{n:>6}\tp4 Payment is due within thirty days.\n")
    );
    let input = format!(r#"{{"command":"view","path":"document.md","view_range":[{n},-1]}}"#);
    let out = run(&mut doc, &command(&input), &stamp()).unwrap();
    assert!(out.text.starts_with(&format!("{n:>6}\tp4")), "{}", out.text);
    assert!(out.text.contains("After the table."), "{}", out.text);
    let bad = run(
        &mut doc,
        &command(r#"{"command":"view","path":"document.md","view_range":[0,2]}"#),
        &stamp(),
    )
    .unwrap_err();
    assert_eq!(bad.error.code(), ErrorCode::InvalidPayload);
}

#[test]
fn a_unique_str_replace_commits_a_tracked_edit_and_shows_the_block() {
    let mut doc = doc();
    let before = doc.base();
    let out = run(
        &mut doc,
        &command(
            r#"{"command":"str_replace","path":"document.md","old_str":"thirty days","new_str":"sixty days"}"#,
        ),
        &stamp(),
    )
    .unwrap();
    assert_ne!(doc.base(), before);
    let n = line_of(&doc, "p4 ");
    assert!(
        out.text.starts_with("document.md was edited: "),
        "{}",
        out.text
    );
    assert!(out.text.contains("tracked change"), "{}", out.text);
    assert!(out.text.contains(&format!("{n:>6}\tp4 ")), "{}", out.text);
    assert!(out.text.contains("sixty"), "{}", out.text);
    let receipt = out.receipt.unwrap();
    assert!(receipt.tracked);
    let revisions = doc
        .revisions(&RevisionFilter {
            author: Some("Clauseo".into()),
            ..RevisionFilter::default()
        })
        .unwrap();
    assert!(!revisions.is_empty());
}

#[test]
fn an_ambiguous_str_replace_lists_every_match_and_changes_nothing() {
    let mut doc = doc();
    // "the " is in several paragraphs.
    let before = doc.base();
    let result = run(
        &mut doc,
        &command(
            r#"{"command":"str_replace","path":"document.md","old_str":"the ","new_str":"a "}"#,
        ),
        &stamp(),
    );
    let failure = result.clone().unwrap_err();
    assert_eq!(failure.error.code(), ErrorCode::AmbiguousMatch);
    assert_eq!(failure.error.file(), Some("document.md"));
    assert!(failure.error.line().is_some());
    assert_eq!(doc.base(), before);
    let (text, is_error) = tool_result(&result);
    assert!(is_error);
    assert!(text.starts_with("Error ambiguous_match: "), "{text}");
    assert!(text.contains("Match 1 at line "), "{text}");
    assert!(text.contains("Match 2 at line "), "{text}");
    assert!(text.contains("\nNext: "), "{text}");
}

#[test]
fn a_missing_old_str_is_not_found() {
    let mut doc = doc();
    let failure = run(
        &mut doc,
        &command(
            r#"{"command":"str_replace","path":"document.md","old_str":"nowhere","new_str":"x"}"#,
        ),
        &stamp(),
    )
    .unwrap_err();
    assert_eq!(failure.error.code(), ErrorCode::NotFound);
}

#[test]
fn insert_adds_a_paragraph_after_a_line() {
    let mut doc = doc();
    let n = line_of(&doc, "p4 Payment");
    let input = format!(
        r#"{{"command":"insert","path":"document.md","insert_line":{n},"insert_text":"Interest accrues daily."}}"#
    );
    let out = run(&mut doc, &command(&input), &stamp()).unwrap();
    assert!(out.text.contains("Interest accrues daily."), "{}", out.text);
    let receipt = out.receipt.unwrap();
    assert_eq!(receipt.added_count, 1, "{}", receipt.text);
    let text = &doc.files().unwrap().files[0].1;
    assert!(text.contains("Interest accrues daily."), "{text}");
}

#[test]
fn insert_takes_new_str_as_well() {
    let input = r#"{"command":"insert","path":"document.md","insert_line":3,"new_str":"x"}"#;
    assert!(matches!(command(input), Command::Insert { insert_text, .. } if insert_text == "x"));
}

#[test]
fn an_edit_the_core_refuses_is_an_error_result_and_changes_nothing() {
    let mut doc = doc();
    let before = doc.base();
    // Changing a ref is not something an edit of files can say: p5 would be on two lines.
    let result = run(
        &mut doc,
        &command(
            r#"{"command":"str_replace","path":"document.md","old_str":"p4 Payment","new_str":"p5 Payment"}"#,
        ),
        &stamp(),
    );
    let (text, is_error) = tool_result(&result);
    assert!(is_error, "{text}");
    assert!(text.starts_with("Error invalid_payload: "), "{text}");
    assert!(text.contains("\nNext: "), "{text}");
    assert_eq!(doc.base(), before);
}

#[test]
fn create_and_undo_are_unsupported_and_unknown_files_are_not_found() {
    let mut doc = doc();
    for input in [
        r#"{"command":"create","path":"new.md","file_text":"x"}"#,
        r#"{"command":"undo_edit","path":"document.md"}"#,
    ] {
        let failure = run(&mut doc, &command(input), &stamp()).unwrap_err();
        assert_eq!(failure.error.code(), ErrorCode::Unsupported, "{input}");
    }
    let failure = run(
        &mut doc,
        &command(r#"{"command":"view","path":"/other.md"}"#),
        &stamp(),
    )
    .unwrap_err();
    assert_eq!(failure.error.code(), ErrorCode::NotFound);
    assert!(failure.error.message().contains("document.md"));
}

#[test]
fn respond_reads_the_input_and_renders_errors_as_text() {
    let mut doc = doc();
    let response = respond(&mut doc, r#"{"command":"rename","path":"x"}"#, &stamp(), "");
    assert!(response.is_error);
    assert!(
        response.text.starts_with("Error invalid_payload: "),
        "{}",
        response.text
    );
    let response = respond(&mut doc, r#"{"command":"view","path":"/"}"#, &stamp(), "");
    assert!(!response.is_error, "{}", response.text);
    assert!(response.receipt.is_none());
}

#[test]
fn an_unpaired_surrogate_is_named_and_changes_nothing() {
    let mut doc = doc();
    let before = doc.base();
    for half in [r"\ud83d", r"\ude00"] {
        let input = format!(
            r#"{{"command":"str_replace","path":"document.md","old_str":"Payment","new_str":"a{half}b"}}"#
        );
        let response = respond(&mut doc, &input, &stamp(), "");
        assert!(response.is_error);
        assert!(
            response.text.starts_with(
                "Error invalid_payload: The text editor command has text with an unpaired surrogate"
            ),
            "{}",
            response.text
        );
        assert!(
            response
                .text
                .contains("Next: Write the text again with whole characters"),
            "{}",
            response.text
        );
    }
    assert_eq!(doc.base(), before);
}

#[test]
fn a_prefix_puts_the_files_in_a_virtual_directory() {
    let mut doc = doc();
    let prefix = "/documents/acme/";
    let list = respond(
        &mut doc,
        r#"{"command":"view","path":"/documents/acme"}"#,
        &stamp(),
        prefix,
    );
    assert!(
        list.text.contains("/documents/acme/document.md ("),
        "{}",
        list.text
    );
    let outside = respond(
        &mut doc,
        r#"{"command":"view","path":"/document.md"}"#,
        &stamp(),
        prefix,
    );
    assert!(
        outside.is_error && outside.text.starts_with("Error not_found: "),
        "{}",
        outside.text
    );
    assert!(
        outside.text.contains("/documents/acme/document.md"),
        "{}",
        outside.text
    );
    let edit = respond(
        &mut doc,
        r#"{"command":"str_replace","path":"/documents/acme/document.md","old_str":"thirty days","new_str":"sixty days"}"#,
        &stamp(),
        "documents/acme",
    );
    assert!(!edit.is_error, "{}", edit.text);
    assert!(
        edit.text
            .starts_with("/documents/acme/document.md was edited: "),
        "{}",
        edit.text
    );
    let receipt = edit.receipt.expect("a committed edit has a receipt");
    assert_eq!(receipt.base, doc.base());
}
