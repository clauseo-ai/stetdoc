//! The row that holds a repeating section's start (issue 27, Word for Mac, decision 0020). Word keeps the start of a
//! control that wraps rows in its first row. Reject All of a tracked first item took the start with the row and removed
//! the section and every item, and Accept All of a tracked deletion of the first item's row did the same to the items
//! left. A new row before the first item is a new first item untracked; tracked it goes above the section and is no
//! item, with a `row_outside_repeating_section` warning. A tracked deletion of the first item's row is written, with a
//! `repeating_section_unwrapped_by_word` warning when rows of the section stay. An item whose row sits in a plain
//! control counts as an item for both.

use super::tests_edit::*;
use super::tests_edit_repeating::{
    delete, document_xml, insert, item, open, outcome, package, repeating, row, table,
};
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use crate::api::view::WarningCode;

fn outside(receipt: &Receipt) -> Vec<(String, String)> {
    receipt
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::RowOutsideRepeatingSection)
        .map(|w| (w.block.clone().unwrap_or_default(), w.message.clone()))
        .collect()
}

#[test]
fn an_untracked_insert_before_the_first_item_row_is_a_new_first_item() {
    let mut doc = open(&repeating());
    let b = batch(&doc, &insert("r1", "before"));
    let receipt = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert_eq!(
        table(&doc),
        "{10 section {13 item [New row.]} {11 item [Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert!(outside(&receipt).is_empty(), "{:?}", receipt.warnings);
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn a_tracked_insert_before_the_first_item_row_goes_above_the_section_and_warns() {
    let source = repeating();
    let mut doc = open(&source);
    let receipt = run(&mut doc, &insert("r1", "before"));
    assert_eq!(
        table(&doc),
        "[+New row.] {10 section {11 item [Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
    let warnings = outside(&receipt);
    assert_eq!(warnings.len(), 1, "{:?}", receipt.warnings);
    let (block, message) = &warnings[0];
    assert_eq!(block, "r1");
    assert!(message.contains("before r1"), "{message}");
    assert!(message.contains("Reject All"), "{message}");
    assert!(doc.check().ok, "{:?}", doc.check());

    // The section's first row is item one's, so rejecting takes no control's start: the source comes back.
    let mut rejected = open(&source);
    run(&mut rejected, &insert("r1", "before"));
    run(&mut rejected, r#"{"op":"reject","all":true}"#);
    assert_eq!(document_xml(&rejected), document_xml(&open(&source)));

    run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(
        table(&doc),
        "[New row.] {10 section {11 item [Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn a_tracked_insert_before_the_second_row_of_the_first_item_goes_above_the_section_too() {
    let source = package(&format!(
        "{}{}",
        item(11, &format!("{}{}", row("Name."), row("Address."))),
        item(12, &row("Item two."))
    ));
    let mut doc = open(&source);
    let receipt = run(&mut doc, &insert("r2", "before"));
    assert_eq!(
        table(&doc),
        "[+New row.] {10 section {11 item [Name.] [Address.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert_eq!(outside(&receipt).len(), 1, "{:?}", receipt.warnings);
}

#[test]
fn a_tracked_insert_before_a_later_item_is_still_a_new_item() {
    let (pending, done) = outcome(&repeating(), &insert("r2", "before"));
    assert_eq!(
        pending,
        "{10 section {11 item [Item one.]} {13 item [+New row.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert_eq!(
        done,
        "{10 section {11 item [Item one.]} {13 item [New row.]} {12 item [Item two.]}} [Plain row.]"
    );
}

#[test]
fn a_tracked_insert_before_the_first_item_after_its_only_row_is_deleted_goes_above_the_section() {
    // Item one's row deleted tracked is still the section's first row until the deletion is accepted.
    let mut doc = open(&repeating());
    let receipt = run(
        &mut doc,
        &format!("{},{}", delete("r1"), insert("r1", "before")),
    );
    assert_eq!(
        table(&doc),
        "[+New row.] {10 section {11 item [-Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert_eq!(outside(&receipt).len(), 1, "{:?}", receipt.warnings);
    assert!(doc.check().ok, "{:?}", doc.check());
}

fn unwrapped(receipt: &Receipt) -> Vec<(String, String)> {
    receipt
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::RepeatingSectionUnwrappedByWord)
        .map(|w| (w.block.clone().unwrap_or_default(), w.message.clone()))
        .collect()
}

#[test]
fn a_tracked_delete_of_the_first_item_row_is_written_and_warns() {
    let (pending, done) = outcome(&repeating(), &delete("r1"));
    assert_eq!(
        pending,
        "{10 section {11 item [-Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert_eq!(done, "{10 section {12 item [Item two.]}} [Plain row.]");
    let mut doc = open(&repeating());
    let warnings = unwrapped(&run(&mut doc, &delete("r1")));
    assert_eq!(warnings.len(), 1);
    let (block, message) = &warnings[0];
    assert_eq!(block, "r1");
    assert!(message.contains("Accept All"), "{message}");
    assert!(message.contains("untracked"), "{message}");
}

#[test]
fn no_unwrap_warning_untracked_for_a_later_item_or_when_every_item_goes() {
    let mut untracked = open(&repeating());
    let b = batch(&untracked, &delete("r1"));
    let receipt = untracked.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(unwrapped(&receipt).is_empty(), "{:?}", receipt.warnings);
    for ops in [delete("r2"), format!("{},{}", delete("r1"), delete("r2"))] {
        let mut doc = open(&repeating());
        let receipt = run(&mut doc, &ops);
        assert!(
            unwrapped(&receipt).is_empty(),
            "{ops}: {:?}",
            receipt.warnings
        );
    }
}

#[test]
fn only_the_first_row_of_a_two_row_first_item_holds_the_start() {
    let source = package(&format!(
        "{}{}",
        item(11, &format!("{}{}", row("Name."), row("Address."))),
        item(12, &row("Item two."))
    ));
    let mut doc = open(&source);
    assert_eq!(unwrapped(&run(&mut doc, &delete("r1"))).len(), 1);
    let mut doc = open(&source);
    assert!(unwrapped(&run(&mut doc, &delete("r2"))).is_empty());
}

/// The review's repro: item 11's row sits in a plain control (13), item 12's does not.
fn nested() -> Vec<u8> {
    package(&format!(
        "{}{}",
        item(
            11,
            &format!(
                r#"<w:sdt><w:sdtPr><w:id w:val="13"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
                row("Item one.")
            )
        ),
        item(12, &row("Item two."))
    ))
}

#[test]
fn an_item_whose_row_sits_in_a_plain_control_is_still_the_first_item() {
    let source = nested();
    let mut doc = open(&source);
    let receipt = run(&mut doc, &insert("r1", "before"));
    assert_eq!(
        table(&doc),
        "[+New row.] {10 section {11 item {13 control [Item one.]}} {12 item [Item two.]}} [Plain row.]"
    );
    assert_eq!(outside(&receipt).len(), 1, "{:?}", receipt.warnings);
    assert!(doc.check().ok, "{:?}", doc.check());
    let mut rejected = open(&source);
    run(&mut rejected, &insert("r1", "before"));
    run(&mut rejected, r#"{"op":"reject","all":true}"#);
    assert_eq!(document_xml(&rejected), document_xml(&open(&source)));

    let mut doc = open(&source);
    let receipt = run(&mut doc, &delete("r1"));
    assert_eq!(unwrapped(&receipt).len(), 1, "{:?}", receipt.warnings);
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(
        table(&doc),
        "{10 section {12 item [Item two.]}} [Plain row.]"
    );
}

#[test]
fn the_view_numbers_a_row_in_a_plain_control_as_its_item() {
    let doc = open(&nested());
    let files = doc.files().unwrap();
    let body = files
        .files
        .iter()
        .find(|f| f.0 == "document.md")
        .map(|f| f.1.clone())
        .unwrap_or_default();
    assert!(body.contains("r1 {row item=1}"), "{body}");
    assert!(body.contains("r2 {row item=2}"), "{body}");
}
