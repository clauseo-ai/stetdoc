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

#[test]
fn another_authors_change_to_a_mark_keeps_the_last_paragraph_and_says_so() {
    let other = r#"<w:rPrChange w:id="9" w:author="Other" w:date="2020-01-01T00:00:00Z"><w:rPr/></w:rPrChange>"#;
    let bold = r#"<w:pPr><w:rPr><w:b/></w:rPr></w:pPr>"#;
    let changed = format!(r#"<w:pPr><w:rPr><w:i/>{other}</w:rPr></w:pPr>"#);
    let two = format!("{}{}", para("Lead.", bold), para("Gone.", &changed));
    let cell = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc>{two}</w:tc></w:tr></w:tbl>{}"#,
        para("After.", "")
    );
    for source in [two.clone(), cell] {
        let mut doc = open(&source);
        let receipt = edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#);
        assert!(
            receipt
                .warnings
                .iter()
                .any(|w| w.code == WarningCode::KeptRequiredParagraph
                    && w.message.contains(
                        "its paragraph mark has another author's pending change to its formatting"
                    )),
            "{}",
            receipt.text
        );
        assert!(body(&doc).contains(other), "{}", body(&doc));
    }
}

#[test]
fn an_insertion_before_the_deleted_last_paragraph_has_its_original_properties() {
    // As in 0.9.3: the new paragraph has the properties of the paragraph it was inserted before, and the paragraph before
    // keeps its own.
    let source = format!("{}{}", para("Lead.", CENTRED), para("Gone.", INDENTED));
    let ops = r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#;
    assert_eq!(
        accepted(&source, ops),
        format!("{}{}", para("Lead.", CENTRED), para("New.", INDENTED))
    );
    let mut doc = open(&source);
    edit(&mut doc, ops);
    edit(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(
        body(&doc).replace("</w:t></w:r><w:r><w:t xml:space=\"preserve\">", ""),
        source
    );
}

#[test]
fn two_insertions_before_the_deleted_last_paragraph_keep_their_order() {
    let source = format!("{}{}", para("Lead.", ""), para("Gone.", ""));
    let ops = r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]},{"op":"insert","ref":"p2","place":"before","paragraphs":["Two."]}"#;
    let mut doc = open(&source);
    let receipt = edit(&mut doc, ops);
    assert!(
        !receipt.text.contains("resolved by a later operation"),
        "{}",
        receipt.text
    );
    // Each insertion lists the paragraph insertion its revisions are in.
    for line in ["1 insert", "2 insert"] {
        assert!(
            receipt
                .text
                .lines()
                .any(|l| l.starts_with(line) && l.contains("paragraph insertion #")),
            "{}",
            receipt.text
        );
    }
    assert_eq!(
        accepted(&source, ops),
        format!(
            "{}{}{}",
            para("Lead.", ""),
            para("New.", ""),
            para("Two.", "")
        )
    );
}

#[test]
fn another_authors_change_to_a_mark_with_nothing_to_carry_does_not_keep_the_paragraph() {
    let other = r#"<w:rPrChange w:id="9" w:author="Other" w:date="2020-01-01T00:00:00Z"><w:rPr/></w:rPrChange>"#;
    let changed = format!(r#"<w:pPr><w:rPr><w:b/>{other}</w:rPr></w:pPr>"#);
    let bold = r#"<w:pPr><w:rPr><w:b/></w:rPr></w:pPr>"#;
    let source = format!("{}{}", para("Lead.", bold), para("Gone.", &changed));
    let mut doc = open(&source);
    let receipt = edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#);
    assert!(
        !receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::KeptRequiredParagraph),
        "{}",
        receipt.text
    );
}

#[test]
fn an_insertion_before_the_deleted_last_paragraph_has_its_original_mark_and_own_changes() {
    let bold = r#"<w:pPr><w:rPr><w:b/></w:rPr></w:pPr>"#;
    let italic = r#"<w:pPr><w:rPr><w:i/></w:rPr></w:pPr>"#;
    let ops = r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#;
    let source = format!("{}{}", para("Lead.", bold), para("Gone.", italic));
    // As in 0.9.3, its text has the mark's run properties too.
    let new = r#"<w:p><w:pPr><w:rPr><w:i/></w:rPr></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">New.</w:t></w:r></w:p>"#;
    assert_eq!(
        accepted(&source, ops),
        format!("{}{new}", para("Lead.", bold))
    );
    // The author's own pending change from an earlier batch, then the delete and the insertion in one batch: the join
    // recorded what the paragraph had, and the new paragraph has it, as in 0.9.3.
    let own = r#"<w:pPr><w:ind w:left="720"/><w:pPrChange w:id="9" w:author="Clauseo" w:date="2020-01-01T00:00:00Z"><w:pPr/></w:pPrChange></w:pPr>"#;
    let source = format!("{}{}", para("Lead.", CENTRED), para("Gone.", own));
    assert_eq!(
        accepted(&source, ops),
        format!("{}{}", para("Lead.", CENTRED), para("New.", INDENTED))
    );
    // So does a `paragraph` operation of the same batch.
    let source = format!("{}{}", para("Lead.", CENTRED), para("Gone.", ""));
    let ops = r#"{"op":"paragraph","ref":"p2","indent_left":"0.5in"},{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#;
    assert_eq!(
        accepted(&source, ops),
        format!("{}{}", para("Lead.", CENTRED), para("New.", INDENTED))
    );
    // Across batches the join folded its alignment into that change, so what the paragraph had is no longer in the file:
    // nothing is guessed, and the new paragraph has the paragraph before's properties (a known difference from 0.9.3).
    let mut doc = open(&source);
    edit(
        &mut doc,
        r#"{"op":"paragraph","ref":"p2","indent_left":"0.5in"}"#,
    );
    edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#);
    edit(
        &mut doc,
        r#"{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#,
    );
    edit(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(
        body(&doc),
        format!("{}{}", para("Lead.", CENTRED), para("New.", CENTRED))
    );
}

#[test]
fn an_insertion_in_a_later_batch_beside_the_deleted_last_paragraph_stays_a_paragraph_of_its_own() {
    let source = format!("{}{}", para("Lead.", ""), para("Gone.", ""));
    for insert in [
        r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#,
        r#"{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#,
    ] {
        let mut doc = open(&source);
        edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#);
        edit(&mut doc, insert);
        edit(&mut doc, r#"{"op":"accept","all":true}"#);
        assert_eq!(
            body(&doc),
            format!("{}{}", para("Lead.", ""), para("New.", "")),
            "{insert}"
        );
    }
}

#[test]
fn another_authors_change_on_the_last_mark_is_not_taken_for_the_joins_alignment() {
    // The soak's case (document 0, sequence 322): the marks' run properties are equal, so the join carries nothing, and the
    // other author's change on the last mark stays with the paragraph that an insertion before it leaves. Reject all gives
    // back the source.
    let other = r#"<w:rPrChange w:id="9" w:author="Other" w:date="2020-01-01T00:00:00Z"><w:rPr/></w:rPrChange>"#;
    let bold = r#"<w:pPr><w:rPr><w:b/></w:rPr></w:pPr>"#;
    let changed = format!(r#"<w:pPr><w:rPr><w:b/>{other}</w:rPr></w:pPr>"#);
    let source = format!("{}{}", para("Lead.", bold), para("Gone.", &changed));
    let mut doc = open(&source);
    edit(
        &mut doc,
        r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#,
    );
    edit(&mut doc, r#"{"op":"reject","author":"Clauseo"}"#);
    assert_eq!(body(&doc), source);
}
