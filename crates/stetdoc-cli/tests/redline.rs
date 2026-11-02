//! The redline a lawyer reads: a replacement never cuts a bracket pair in half, so `thirty (30)` to `forty-five (45)`
//! strikes the whole `thirty (30)` and inserts the whole `forty-five (45)`, with no `)` left standing after the
//! insertion (design `revisions.md`, 2.5 rule 5).
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

fn with_paragraph(text: &str) -> tempfile::TempDir {
    let body = format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>");
    let doc = parts::Doc::new().body(&body).bytes();
    let tmp = tempfile::tempdir().unwrap();
    fs::write(
        tmp.path().join("p.docx"),
        Zip::new(parts::entries_doc(&doc)).build(),
    )
    .unwrap();
    tmp
}

/// The receipt's line for p1.
fn p1(receipt: &str) -> &str {
    receipt
        .lines()
        .find(|l| l.starts_with("p1 "))
        .unwrap_or_else(|| panic!("no p1 line in {receipt}"))
}

#[test]
fn a_number_in_words_and_brackets_is_struck_and_inserted_whole_however_wide_the_search() {
    let tmp = with_paragraph("Payment shall be due within thirty (30) days of invoice.");
    let want = "p1 Payment shall be due within {--thirty (30)--}{#0 JD}{++forty-five (45)++}{#1 JD} days of invoice.";
    for (old, new) in [
        ("thirty (30)", "forty-five (45)"),
        (
            "due within thirty (30) days",
            "due within forty-five (45) days",
        ),
    ] {
        let receipt = out(&stetdoc(
            tmp.path(),
            &["replace", "p.docx", old, new, "--dry-run"],
        ));
        assert_eq!(p1(&receipt), want, "{old:?}");
    }
}

#[test]
fn glued_punctuation_that_is_not_a_bracket_stays_outside_the_edit_as_in_words_compare() {
    let tmp = with_paragraph("Interest accrues at 2% per month on the Seller's invoices.");
    let receipt = out(&stetdoc(
        tmp.path(),
        &[
            "replace",
            "p.docx",
            "at 2% per",
            "at 3% per",
            "the Seller's",
            "the Vendor's",
            "--dry-run",
        ],
    ));
    let line = p1(&receipt);
    assert!(line.contains("{--2--}"), "{line}");
    assert!(line.contains("{++3++}"), "{line}");
    assert!(line.contains("{--Seller--}"), "{line}");
    assert!(line.contains("{++Vendor++}"), "{line}");
}
