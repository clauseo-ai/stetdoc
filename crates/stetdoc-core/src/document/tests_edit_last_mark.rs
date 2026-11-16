//! The tracked delete of a container's last paragraph (issue #25, design `revisions.md` 3.2.2), next to other operations of
//! the batch and in the Final view: the last paragraph keeps its mark and the paragraph before it loses its own, because
//! Word cannot accept a deleted mark on a container's last paragraph.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use crate::api::view::{ViewSpec, WarningCode};
use stetdoc_testkit::fixtures::parts;
use stetdoc_testkit::fixtures::zip::Zip;

fn para(text: &str, ppr: &str) -> String {
    format!(r#"<w:p>{ppr}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

fn open(body: &str) -> Document {
    let xml = parts::Doc::new().body(body).bytes();
    Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

fn body(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let xml = String::from_utf8(view.tree(id).unwrap().serialize()).unwrap();
    let start = xml.find("<w:body>").unwrap() + "<w:body>".len();
    let end = xml.rfind("<w:sectPr/></w:body>").unwrap();
    xml[start..end].replace("<w:t>", r#"<w:t xml:space="preserve">"#)
}

fn edit(doc: &mut Document, ops: &str) -> Receipt {
    let receipt = doc
        .edit(Edit::ops(batch(doc, ops), stamp()))
        .unwrap_or_else(|err| panic!("{ops}: {err:?}"));
    assert!(doc.check().ok, "{ops}: {:?}", doc.check());
    receipt
}

fn accepted(source: &str, ops: &str) -> String {
    let mut doc = open(source);
    edit(&mut doc, ops);
    edit(&mut doc, r#"{"op":"accept","all":true}"#);
    body(&doc)
}

fn final_view(doc: &Document) -> String {
    let spec = ViewSpec {
        mode: crate::api::view::Mode::Final,
        ..ViewSpec::default()
    };
    doc.view(&spec).unwrap().text
}

const CENTRED: &str = r#"<w:pPr><w:jc w:val="center"/></w:pPr>"#;
const INDENTED: &str = r#"<w:pPr><w:ind w:left="720"/></w:pPr>"#;

#[test]
fn deleting_the_last_two_paragraphs_from_the_bottom_up_is_the_same_as_from_the_top_down() {
    let a = para("A", CENTRED);
    let three = format!("{a}{}{}", para("B", INDENTED), para("C", ""));
    let cell = |inner: &str| {
        format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc>{inner}</w:tc></w:tr></w:tbl>{}"#,
            para("After.", "")
        )
    };
    for (source, want) in [(three.clone(), a.clone()), (cell(&three), cell(&a))] {
        let mut doc = open(&source);
        let receipt = edit(
            &mut doc,
            r#"{"op":"delete","ref":"p3"},{"op":"delete","ref":"p2"}"#,
        );
        assert!(
            !receipt.text.contains("resolved by a later operation"),
            "{}",
            receipt.text
        );
        let bottom_up = accepted(
            &source,
            r#"{"op":"delete","ref":"p3"},{"op":"delete","ref":"p2"}"#,
        );
        assert_eq!(bottom_up, want);
        assert_eq!(
            accepted(
                &source,
                r#"{"op":"delete","ref":"p2"},{"op":"delete","ref":"p3"}"#
            ),
            want
        );
        assert_eq!(
            accepted(&source, r#"{"op":"delete","ref":"p2","to":"p3"}"#),
            want
        );
    }
}

#[test]
fn an_insertion_before_the_deleted_last_paragraph_stays_a_paragraph_of_its_own() {
    let source = format!("{}{}", para("Lead.", ""), para("Gone.", ""));
    let got = accepted(
        &source,
        r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#,
    );
    assert_eq!(got, format!("{}{}", para("Lead.", ""), para("New.", "")));
}

#[test]
fn the_final_view_is_unchanged_for_a_range_deleted_to_the_end() {
    // The XML is what 0.9.3 wrote: the last paragraph is the body's only one left, kept with its mark, and it is the one
    // the view shows and an edit can name.
    let source = format!("{}{}{}", para("A", ""), para("B", ""), para("C", ""));
    let mut doc = open(&source);
    edit(&mut doc, r#"{"op":"delete","ref":"p1","to":"p3"}"#);
    let view = final_view(&doc);
    assert!(view.lines().any(|l| l == "p3"), "{view}");
    assert!(!view.lines().any(|l| l.starts_with("p2")), "{view}");
    edit(
        &mut doc,
        r#"{"op":"replace","ref":"p3","start":0,"end":0,"with":"Z"}"#,
    );
}

#[test]
fn deleting_the_last_two_lines_reads_back_as_the_first_one() {
    let source = format!("{}{}{}", para("A", ""), para("B", ""), para("C", ""));
    for ops in [
        r#"{"op":"delete","ref":"p2"},{"op":"delete","ref":"p3"}"#,
        r#"{"op":"delete","ref":"p3"},{"op":"delete","ref":"p2"}"#,
    ] {
        let mut doc = open(&source);
        edit(&mut doc, ops);
        let view = final_view(&doc);
        assert!(view.lines().any(|l| l == "p1 A"), "{ops}: {view}");
        assert!(
            !view
                .lines()
                .any(|l| l.starts_with("p2") || l.starts_with("p3")),
            "{ops}: {view}"
        );
    }
}

#[test]
fn accepting_keeps_the_run_properties_of_the_earlier_paragraphs_mark() {
    let bold = r#"<w:pPr><w:rPr><w:b/></w:rPr></w:pPr>"#;
    let italic = r#"<w:pPr><w:rPr><w:i/></w:rPr></w:pPr>"#;
    let source = format!("{}{}", para("Lead.", bold), para("Gone.", italic));
    let got = accepted(&source, r#"{"op":"delete","ref":"p2"}"#);
    assert_eq!(got, para("Lead.", bold));
}

#[test]
fn a_last_paragraph_after_an_empty_one_is_kept_and_the_receipt_says_why() {
    let source = format!(r#"<w:p>{CENTRED}</w:p>{}"#, para("Last.", INDENTED));
    let mut doc = open(&source);
    let receipt = edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#);
    let warning = receipt
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::KeptRequiredParagraph)
        .unwrap_or_else(|| panic!("{}", receipt.text));
    assert!(
        warning.message.contains("the paragraph before it is empty"),
        "{}",
        warning.message
    );
    edit(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(
        body(&doc),
        format!(r#"<w:p>{CENTRED}</w:p><w:p>{INDENTED}</w:p>"#)
    );
}

#[test]
fn a_kept_last_paragraph_names_the_change_that_stops_the_join() {
    let other = r#"<w:pPrChange w:id="9" w:author="Other" w:date="2020-01-01T00:00:00Z"><w:pPr/></w:pPrChange>"#;
    let changed = format!(r#"<w:pPr><w:jc w:val="center"/>{other}</w:pPr>"#);
    for (source, cause) in [
        (
            format!("{}{}", para("Lead.", ""), para("Gone.", &changed)),
            "because it has another author's pending change to its properties",
        ),
        (
            format!("{}{}", para("Lead.", &changed), para("Gone.", "")),
            "because the paragraph before it has another author's pending change to its properties",
        ),
    ] {
        let mut doc = open(&source);
        let receipt = edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#);
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.code == WarningCode::KeptRequiredParagraph && w.message.contains(cause)),
            "{}",
            receipt.text
        );
    }
}

#[test]
fn a_range_that_ends_at_the_start_of_an_empty_last_paragraph() {
    // From the start of p2, p2 and the empty p3 go, and p1's mark is the one deleted. From inside p2, p2's text is what is
    // left last, so p1 is not touched.
    let source = format!("{}{}<w:p/>", para("A", ""), para("Bee", ""));
    let mut doc = open(&source);
    edit(
        &mut doc,
        r#"{"op":"delete","ref":"p2","to":"p3","start":0,"end":0}"#,
    );
    edit(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(body(&doc), para("A", ""));
    let mut doc = open(&source);
    edit(
        &mut doc,
        r#"{"op":"delete","ref":"p2","to":"p3","start":1,"end":0}"#,
    );
    assert!(body(&doc).starts_with(&para("A", "")), "{}", body(&doc));
    edit(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(body(&doc), format!("{}{}", para("A", ""), para("B", "")));
}
