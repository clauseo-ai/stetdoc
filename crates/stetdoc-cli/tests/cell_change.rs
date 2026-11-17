//! A cell's property change (`w:tcPrChange`) in the working copy: the view shows it as `{rev cell #id AUTH}` on the cell's
//! first paragraph, and a copy with or without that tag packs (decision 0023).
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

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// A table whose first cell has a shading change by Bob (revision 112), beside a plain cell.
fn package(cwd: &Path) {
    let para = |t: &str| format!("<w:p><w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r></w:p>");
    let changed = format!(
        "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/><w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"FFFF00\"/>\
         <w:tcPrChange w:id=\"112\" w:author=\"Bob\" w:date=\"2024-01-01T00:00:00Z\"><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr>\
         </w:tcPrChange></w:tcPr>{}</w:tc>",
        para("CELL-FMT")
    );
    let plain = format!(
        "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>",
        para("Plain cell")
    );
    let table = format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>\
         <w:tr>{changed}{plain}</w:tr></w:tbl>"
    );
    let body = format!(
        "{}{table}{}",
        para("Intro sentence before the table."),
        para("Closing words after the table.")
    );
    let doc = parts::Doc::new().body(&body).bytes();
    fs::write(
        cwd.join("cell.docx"),
        Zip::new(parts::entries_doc(&doc)).build(),
    )
    .unwrap();
}

/// The working copy of `cell.docx`, unpacked by this build.
fn unpacked() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    package(tmp.path());
    let out = stetdoc(tmp.path(), &["unpack", "cell.docx"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    tmp
}

fn document_md(cwd: &Path) -> String {
    fs::read_to_string(cwd.join("cell.stet/document.md")).unwrap()
}

fn rewrite(cwd: &Path, from: &str, to: &str) {
    let path = cwd.join("cell.stet/document.md");
    let doc = document_md(cwd);
    assert!(doc.contains(from), "{doc}");
    fs::write(&path, doc.replace(from, to)).unwrap();
}

#[test]
fn the_view_marks_the_cell_on_its_first_paragraph_and_the_ledger_lists_its_id() {
    let tmp = tempfile::tempdir().unwrap();
    package(tmp.path());
    let view = stetdoc(tmp.path(), &["view", "cell.docx"]);
    let said = text(&view.stdout);
    assert!(said.contains("\np2 {rev cell #112 B} CELL-FMT\n"), "{said}");
    assert!(said.contains("\np3 Plain cell\n"), "{said}");

    let ledger = text(&stetdoc(tmp.path(), &["view", "cell.docx", "--revisions"]).stdout);
    assert!(ledger.contains("#112 format B"), "{ledger}");
}

#[test]
fn a_working_copy_packs_with_the_cell_tag_and_without_it() {
    let tmp = unpacked();
    let cwd = tmp.path();
    assert!(document_md(cwd).contains("p2 {rev cell #112 B} CELL-FMT"));

    // As unpacked: an edit of another paragraph packs.
    rewrite(cwd, "Intro sentence", "Opening sentence");
    let packed = stetdoc(cwd, &["pack", "cell.stet", "-o", "edited.docx"]);
    assert!(packed.status.success(), "{}", text(&packed.stderr));

    // A copy as a release before the tag wrote it: no tag on the line. Packs, with or without an edit.
    let older = tempfile::tempdir().unwrap();
    package(older.path());
    assert!(
        stetdoc(older.path(), &["unpack", "cell.docx"])
            .status
            .success()
    );
    rewrite(older.path(), "p2 {rev cell #112 B} CELL-FMT", "p2 CELL-FMT");
    let plain = stetdoc(older.path(), &["pack", "cell.stet", "-o", "plain.docx"]);
    assert!(plain.status.success(), "{}", text(&plain.stderr));
    rewrite(older.path(), "Intro sentence", "Opening sentence");
    let edited = stetdoc(older.path(), &["pack", "cell.stet", "-o", "edited.docx"]);
    assert!(edited.status.success(), "{}", text(&edited.stderr));
    let after = text(&stetdoc(older.path(), &["view", "edited.docx", "--revisions"]).stdout);
    assert!(after.contains("#112 format B"), "{after}");
}

#[test]
fn a_changed_cell_tag_is_refused_as_any_read_only_tag_is() {
    let tmp = unpacked();
    let cwd = tmp.path();
    rewrite(cwd, "{rev cell #112 B}", "{rev cell #999 B}");
    let out = stetdoc(cwd, &["pack", "cell.stet", "-o", "out.docx"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).starts_with("error[readonly_region]"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn accepting_the_change_by_its_id_takes_the_tag_away() {
    let tmp = unpacked();
    let cwd = tmp.path();
    fs::write(
        cwd.join("ops.json"),
        r#"{"ops":[{"op":"accept","ids":[112]}]}"#,
    )
    .unwrap();
    let out = stetdoc(
        cwd,
        &[
            "edit",
            "cell.docx",
            "--ops",
            "ops.json",
            "-o",
            "accepted.docx",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let view = text(&stetdoc(cwd, &["view", "accepted.docx"]).stdout);
    assert!(!view.contains("{rev cell"), "{view}");
    assert!(view.contains("\np2 CELL-FMT\n"), "{view}");
}
