//! `--clause` given more than once: `view` shows the union of the clauses' blocks in document order, each once, and
//! `replace` looks only in those blocks (decision 0012).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use stetdoc_testkit::fixtures::{parts, zip::Zip};

fn stetdoc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env("STETDOC_AUTHOR", "Jane Doe")
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .output()
        .expect("run stetdoc")
}

fn out(o: &Output) -> String {
    assert!(
        o.status.success(),
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// Three clauses, each with "30 days" once: p1 to p6.
fn with_clauses() -> tempfile::TempDir {
    let body = [
        p("1. Payment"),
        p("The Buyer pays within 30 days."),
        p("2. Delivery"),
        p("The Seller delivers within 30 days."),
        p("3. Notices"),
        p("A notice takes effect after 30 days."),
    ]
    .concat();
    let doc = parts::Doc::new().body(&body).bytes();
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("clauses.docx"),
        Zip::new(parts::entries_doc(&doc)).build(),
    )
    .unwrap();
    tmp
}

fn refs(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|l| l.split_once(' ').map(|(r, _)| r))
        .filter(|r| r.starts_with('p') && r[1..].chars().all(|c| c.is_ascii_digit()))
        .collect()
}

#[test]
fn view_takes_clause_more_than_once_and_shows_the_union_in_document_order() {
    let tmp = with_clauses();
    let cwd = tmp.path();
    let text = out(&stetdoc(
        cwd,
        &["view", "clauses.docx", "--clause", "3", "--clause", "1"],
    ));
    assert_eq!(refs(&text), ["p1", "p2", "p5", "p6"], "{text}");
    assert!(text.contains("of clauses 1, 3 ("), "{text}");
    // The same clause twice, or once alone, is the view of one clause.
    let twice = out(&stetdoc(
        cwd,
        &[
            "view",
            "clauses.docx",
            "--clause",
            "2",
            "--clause",
            "clause 2",
        ],
    ));
    assert_eq!(refs(&twice), ["p3", "p4"], "{twice}");
    assert!(twice.contains("of clause 2 ("), "{twice}");
    // A clause that is not there fails the view, and the error names it.
    let miss = stetdoc(
        cwd,
        &["view", "clauses.docx", "--clause", "1", "--clause", "7"],
    );
    assert!(!miss.status.success());
    let err = String::from_utf8_lossy(&miss.stderr);
    assert!(err.contains("not_found") && err.contains("\"7\""), "{err}");
}

#[test]
fn view_pages_across_several_clauses_with_the_cursor_it_prints() {
    let tmp = with_clauses();
    let cwd = tmp.path();
    let args = [
        "view",
        "clauses.docx",
        "--clause",
        "1",
        "--clause",
        "3",
        "--budget",
        "1",
    ];
    let first = out(&stetdoc(cwd, &args));
    assert!(
        first.contains("--clause 1 --clause 3 --budget 1 --after"),
        "{first}"
    );
    let mut seen: Vec<String> = refs(&first).into_iter().map(String::from).collect();
    let mut page = first;
    for _ in 0..10 {
        let Some(after) = page
            .lines()
            .find(|l| l.starts_with("-- more:"))
            .and_then(|l| l.split("--after ").nth(1))
            .and_then(|s| s.split_whitespace().next())
            .map(String::from)
        else {
            break;
        };
        let mut next = args.to_vec();
        next.extend(["--after", after.as_str()]);
        page = out(&stetdoc(cwd, &next));
        seen.extend(refs(&page).into_iter().map(String::from));
    }
    assert_eq!(seen, ["p1", "p2", "p5", "p6"]);
}

#[test]
fn replace_takes_clause_more_than_once_and_looks_only_in_those_clauses() {
    let tmp = with_clauses();
    let cwd = tmp.path();
    // "30 days" is in both clauses, so it is not once where replace looks, and the error names where that is.
    let refused = stetdoc(
        cwd,
        &[
            "replace",
            "clauses.docx",
            "--clause",
            "2",
            "--clause",
            "3",
            "30 days",
            "60 days",
            "--dry-run",
        ],
    );
    assert!(!refused.status.success());
    let err = String::from_utf8_lossy(&refused.stderr);
    assert!(err.contains("in clauses 2, 3"), "{err}");
    out(&stetdoc(
        cwd,
        &[
            "replace",
            "clauses.docx",
            "--clause",
            "2",
            "--clause",
            "3",
            "after 30 days",
            "after 60 days",
            "-o",
            "edited.docx",
        ],
    ));
    let edited = out(&stetdoc(cwd, &["view", "edited.docx", "--mode", "final"]));
    assert!(edited.contains("after 60 days"), "{edited}");
    assert!(edited.contains("pays within 30 days"), "{edited}");
}
