//! Manual markup through `Document::edit` (`Markup::Manual`, decision 0010): old text stays struck through, new text goes in
//! underlined, as direct formatting, and no revision is written. Everything the edit does not change keeps its bytes.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::options::Markup;
use crate::api::receipt::Receipt;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

fn main_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

fn manual(doc: &mut Document, ops: &str, markup: Markup) -> Result<Receipt, StetError> {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, stamp()).markup(markup))
}

/// No revision of any kind in the main part.
fn no_revisions(xml: &str) {
    for tag in ["<w:ins ", "<w:del ", "Change ", "<w:moveFrom", "<w:moveTo"] {
        assert!(!xml.contains(tag), "{tag} in {xml}");
    }
}

const P4: &str = "<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p>";

#[test]
fn a_replace_strikes_the_old_word_underlines_the_new_and_leaves_every_other_byte() {
    let mut doc = rich();
    let before = main_xml(&doc);
    assert!(before.contains(P4), "{before}");
    let receipt = manual(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty days","with":"sixty days"}"#,
        Markup::Manual,
    )
    .unwrap();
    let marked = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Payment is due within </w:t></w:r>"#,
        r#"<w:r><w:rPr><w:strike/></w:rPr><w:t>thirty</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>sixty</w:t></w:r>"#,
        r#"<w:r><w:t xml:space="preserve"> days.</w:t></w:r></w:p>"#,
    );
    let after = main_xml(&doc);
    assert_eq!(after, before.replace(P4, marked));
    no_revisions(&after);
    assert!(receipt.revisions.is_empty(), "{:?}", receipt.revisions);
    assert_eq!(receipt.markup, Markup::Manual);
    assert!(!receipt.tracked);
    assert!(receipt.text.contains("manual markup"), "{}", receipt.text);
    assert!(
        receipt
            .text
            .contains("0 replace p4: struck \"thirty\", underlined \"sixty\"\n"),
        "{}",
        receipt.text
    );
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn in_colour_the_deleted_text_is_red_and_the_new_text_blue() {
    let mut doc = rich();
    let before = main_xml(&doc);
    manual(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
        Markup::ManualColor,
    )
    .unwrap();
    let marked = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Payment is due within </w:t></w:r>"#,
        r#"<w:r><w:rPr><w:strike/><w:color w:val="FF0000"/></w:rPr><w:t>thirty</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:color w:val="0000FF"/><w:u w:val="single"/></w:rPr><w:t>sixty</w:t></w:r>"#,
        r#"<w:r><w:t xml:space="preserve"> days.</w:t></w:r></w:p>"#,
    );
    assert_eq!(main_xml(&doc), before.replace(P4, marked));
}

#[test]
fn a_deletion_is_struck_text_only_and_an_insertion_underlined_text_only() {
    let mut doc = rich();
    manual(
        &mut doc,
        r#"{"op":"delete","ref":"p4","find":" thirty"},{"op":"replace","ref":"p5","find":"English","with":"English and Welsh"}"#,
        Markup::Manual,
    )
    .unwrap();
    let xml = main_xml(&doc);
    no_revisions(&xml);
    assert!(
        xml.contains(
            r#"<w:r><w:rPr><w:strike/></w:rPr><w:t xml:space="preserve"> thirty</w:t></w:r>"#
        ),
        "{xml}"
    );
    assert!(
        !xml.contains(r#"<w:u "#) || xml.contains("and Welsh"),
        "{xml}"
    );
    // The insertion keeps every old word: only the new ones are underlined, and nothing is struck in p5.
    assert!(
        xml.contains(r#"<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t xml:space="preserve"> and Welsh</w:t></w:r>"#),
        "{xml}"
    );
    assert_eq!(xml.matches("<w:strike/>").count(), 1, "{xml}");
    assert_eq!(final_text(&doc, "p4"), "Payment is due within thirty days.");
    assert_eq!(
        final_text(&doc, "p5"),
        "This Agreement is governed by English and Welsh law."
    );
}

#[test]
fn a_deleted_paragraph_stays_struck_and_a_new_one_is_underlined() {
    let mut doc = rich();
    let receipt = manual(
        &mut doc,
        r#"{"op":"delete","ref":"p4"},{"op":"insert","ref":"p5","place":"after","paragraphs":["A new clause."]},{"op":"delete","ref":"p1","start":4,"to":"p2","end":8}"#,
        Markup::Manual,
    )
    .unwrap();
    let xml = main_xml(&doc);
    no_revisions(&xml);
    assert!(
        xml.contains(r#"<w:p><w:r><w:rPr><w:strike/></w:rPr><w:t>Payment is due within thirty days.</w:t></w:r></w:p>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>A new clause.</w:t></w:r>"#),
        "{xml}"
    );
    // A range across paragraphs is struck in both, and the two stay apart: nothing is merged.
    assert_eq!(
        final_text(&doc, "p1"),
        "The Supplier shall indemnify the Customer, and shall pay on demand."
    );
    assert!(
        xml.contains(r#"<w:r><w:rPr><w:strike/></w:rPr><w:t>Supplier shall indemnify the Customer, and shall pay on demand.</w:t></w:r>"#),
        "{xml}"
    );
    assert_eq!(receipt.added_count, 1);
    let marks = |i: usize| -> Vec<(Option<&str>, &str, &str)> {
        receipt.operations[i]
            .marked
            .iter()
            .map(|m| (m.block.as_deref(), m.struck.as_str(), m.underlined.as_str()))
            .collect()
    };
    assert_eq!(
        marks(0),
        [(Some("p4"), "Payment is due within thirty days.", "")]
    );
    assert_eq!(marks(1), [(Some("p11"), "", "A new clause.")]);
    assert_eq!(
        marks(2),
        [
            (
                Some("p1"),
                "Supplier shall indemnify the Customer, and shall pay on demand.",
                ""
            ),
            (Some("p2"), "\u{201c}Losses\u{201d}", ""),
        ]
    );
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn split_merge_row_and_a_table_deletion_are_refused_and_change_nothing() {
    for ops in [
        r#"{"op":"split","ref":"p4","at":4}"#,
        r#"{"op":"merge","ref":"p4"}"#,
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
        r#"{"op":"delete","ref":"t1"}"#,
    ] {
        let mut doc = rich();
        let before = doc.save().unwrap().bytes;
        let e = manual(&mut doc, ops, Markup::Manual).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{ops}: {e}");
        assert!(e.message().contains("manual markup"), "{ops}: {e}");
        assert!(e.next().contains("tracked edit"), "{ops}: {e}");
        assert_eq!(doc.save().unwrap().bytes, before, "{ops}");
    }
}

#[test]
fn a_dry_run_says_manual_and_changes_nothing() {
    let mut doc = rich();
    let before = doc.save().unwrap().bytes;
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let receipt = doc
        .edit(Edit::ops(b, stamp()).markup(Markup::Manual).dry_run())
        .unwrap();
    assert!(receipt.dry_run);
    assert_eq!(receipt.markup, Markup::Manual);
    assert!(receipt.text.contains("manual markup"), "{}", receipt.text);
    assert_eq!(doc.save().unwrap().bytes, before);
}

#[test]
fn the_receipt_json_names_manual_markup_and_leaves_it_out_for_revisions() {
    let mut doc = rich();
    let r = manual(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
        Markup::ManualColor,
    )
    .unwrap();
    let json = serde_json::to_value(&r).unwrap();
    assert_eq!(json["markup"], "manual_color");
    let tracked = run(
        &mut rich(),
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    assert!(
        serde_json::to_value(&tracked)
            .unwrap()
            .get("markup")
            .is_none()
    );
}

/// p1 holds Opposing's insertion between plain text, and p2 Opposing's deletion.
fn with_changes() -> Document {
    let d = r#"w:date="2026-01-01T00:00:00Z""#;
    let body = format!(
        concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">Plain start </w:t></w:r><w:ins w:id="101" w:author="Opposing" {d}><w:r><w:t>inserted words</w:t></w:r></w:ins><w:r><w:t xml:space="preserve"> plain end.</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Price </w:t></w:r><w:del w:id="102" w:author="Opposing" {d}><w:r><w:delText>gross</w:delText></w:r></w:del><w:r><w:t xml:space="preserve"> net.</w:t></w:r></w:p>"#,
        ),
        d = d
    );
    let xml = parts::Doc::new().body(&body).bytes();
    Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

#[test]
fn existing_tracked_changes_stay_as_they_were_and_text_inside_another_authors_is_refused() {
    let mut doc = with_changes();
    let revisions = |doc: &Document| doc.revisions(&Default::default()).unwrap().len();
    assert_eq!(revisions(&doc), 2);
    manual(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"plain end","with":"plain finish"},{"op":"replace","ref":"p2","find":"net","with":"total"}"#,
        Markup::Manual,
    )
    .unwrap();
    let xml = main_xml(&doc);
    assert_eq!(revisions(&doc), 2);
    assert!(
        xml.contains(r#"<w:ins w:id="101" w:author="Opposing" w:date="2026-01-01T00:00:00Z"><w:r><w:t>inserted words</w:t></w:r></w:ins>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<w:del w:id="102" w:author="Opposing" w:date="2026-01-01T00:00:00Z"><w:r><w:delText>gross</w:delText></w:r></w:del>"#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<w:r><w:rPr><w:strike/></w:rPr><w:t xml:space="preserve">end</w:t></w:r><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t xml:space="preserve">finish</w:t></w:r>"#),
        "{xml}"
    );
    assert!(doc.check().ok, "{:?}", doc.check());

    let mut doc = with_changes();
    let e = manual(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"inserted","with":"added"}"#,
        Markup::Manual,
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{e}");
    assert!(e.message().contains("Opposing"), "{e}");
}
