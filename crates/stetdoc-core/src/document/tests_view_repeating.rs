//! The rows of a repeating section's items in the view and the readable files (issue 27, decision 0019): an item's row
//! line shows `item=N`, which is shown only. A file written before the token existed, with plain row lines, still applies,
//! and a file that shows a number must show the right one.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::files::ReadableFiles;
use crate::api::view::ViewSpec;
use crate::refs::{hex, sha256};
use stetdoc_testkit::fixtures::{parts, zip::Zip};

const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

fn row(text: &str) -> String {
    format!(r#"<w:tr><w:tc><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc></w:tr>"#)
}

fn item(id: u32, rows: &str) -> String {
    format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="{id}"/><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent>{rows}</w:sdtContent></w:sdt>"#
    )
}

/// A table with a repeating section of two items, the first with two rows, then a plain row.
fn repeating() -> Document {
    let section = format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="10"/><w15:repeatingSection/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt>"#,
        item(11, &format!("{}{}", row("Name."), row("Address."))),
        item(12, &row("Item two."))
    );
    let body = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4800"/></w:tblGrid>{section}{}</w:tbl><w:p/>"#,
        row("Plain row.")
    );
    let xml = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:w15="{W15}""#))
        .body(&body)
        .bytes();
    Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

fn view(doc: &Document) -> String {
    doc.view(&ViewSpec::default()).unwrap().text
}

/// The files with `from` replaced by `to` in `document.md`, every occurrence.
fn edited(doc: &Document, from: &str, to: &str) -> ReadableFiles {
    let mut files = doc.files().unwrap();
    let text = &mut files.files[0].1;
    assert!(text.contains(from), "{from} is not in\n{text}");
    *text = text.replace(from, to);
    files
}

/// `document.md` as a build before decision 0019 wrote it: no `item=` in a row line.
fn without_items(files: &mut ReadableFiles) {
    let text = &mut files.files[0].1;
    for n in 1..=3 {
        *text = text.replace(&format!(" item={n}"), "");
    }
}

#[test]
fn an_item_row_shows_its_item_number_and_a_plain_row_shows_none() {
    let text = view(&repeating());
    for line in [
        "\nr1 {row item=1}\n",
        "\nr2 {row item=1}\n",
        "\nr3 {row item=2}\n",
        "\nr4 {row}\n",
    ] {
        assert!(text.contains(line), "{line:?} is not in\n{text}");
    }
}

#[test]
fn an_inserted_item_row_shows_its_number_before_the_change() {
    let mut doc = repeating();
    run(
        &mut doc,
        r#"{"op":"row","ref":"r3","action":"insert","place":"after","cells":["New."]}"#,
    );
    let text = view(&doc);
    assert!(
        text.contains("{row item=3 inserted #"),
        "no inserted third item in\n{text}"
    );
}

#[test]
fn a_file_with_no_item_numbers_gives_no_operation_and_its_edits_apply() {
    let mut doc = repeating();
    let mut files = doc.files().unwrap();
    without_items(&mut files);
    let receipt = doc.edit(Edit::files(files, stamp())).unwrap();
    assert!(receipt.operations.is_empty(), "{:?}", receipt.operations);
    let mut files = edited(&doc, "Item two.", "Item 2.");
    without_items(&mut files);
    assert!(!files.files[0].1.contains("item="));
    let receipt = doc.edit(Edit::files(files, stamp())).unwrap();
    assert_eq!(receipt.operations.len(), 1);
    assert_eq!(receipt.operations[0].op, "replace");
    assert!(view(&doc).contains("Item {--two--}"), "{}", view(&doc));
}

#[test]
fn a_file_with_item_numbers_applies_its_edits_and_a_row_line_removed_whole_deletes_the_row() {
    let mut doc = repeating();
    let mut files = doc.files().unwrap();
    let text = files.files[0].1.clone();
    let start = text.find("r3 {row item=2}\n").unwrap();
    let end = text.find("r4 {row}").unwrap();
    files.files[0].1 = format!("{}{}", &text[..start], &text[end..]);
    let receipt = doc.edit(Edit::files(files, stamp())).unwrap();
    assert_eq!(receipt.operations[0].op, "row");
}

#[test]
fn a_changed_or_added_item_number_is_refused() {
    let mut doc = repeating();
    let files = edited(&doc, "r3 {row item=2}", "r3 {row item=5}");
    let e = doc.edit(Edit::files(files, stamp())).err().unwrap();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{}", e.message());
    let files = edited(&doc, "r4 {row}", "r4 {row item=3}");
    assert!(doc.edit(Edit::files(files, stamp())).is_err());
}

#[test]
fn a_working_copy_that_an_earlier_release_wrote_packs_and_one_that_renders_otherwise_is_stale() {
    // What 0.9.3 wrote: row lines with no item number, and the hash of that text as the files' rendering.
    let mut doc = repeating();
    let mut files = doc.files().unwrap();
    without_items(&mut files);
    files.rendered = vec![(
        "document.md".to_owned(),
        hex(&sha256(files.files[0].1.as_bytes())),
    )];
    let mut edited = files.clone();
    edited.files[0].1 = edited.files[0].1.replace("Item two.", "Item 2.");
    let receipt = doc.edit(Edit::files(edited, stamp())).unwrap();
    assert_eq!(receipt.operations[0].op, "replace");
    // A rendering that neither this release nor an earlier one wrote is still release skew.
    let mut other = repeating();
    let mut files = other.files().unwrap();
    files.rendered = vec![("document.md".to_owned(), hex(&sha256(b"something else")))];
    files.files[0].1 = files.files[0].1.replace("Item two.", "Item 2.");
    let e = other.edit(Edit::files(files, stamp())).err().unwrap();
    assert_eq!(e.code(), ErrorCode::StaleFiles, "{}", e.message());
}
