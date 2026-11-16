//! A new row before the first item of a repeating section (issue 27, Word for Mac). Untracked it is a new first item.
//! Tracked it goes above the section and is no item, with a `row_outside_repeating_section` warning, because Word
//! keeps the start of a control that wraps rows in its first row: Reject All of a tracked first item took the start
//! with the row and removed the section and every item (decision 0020). A tracked row before any later item, or after
//! the first, is still a new item.

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
