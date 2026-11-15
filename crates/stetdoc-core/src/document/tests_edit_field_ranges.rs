//! A range that starts before a complex field and ends where its result ends holds the whole field, as does a replace with
//! no range on a paragraph that ends with a field: the field is deleted and the new text goes where it was, tracked and then
//! accepted, and untracked (issue #23). A range that is exactly a field's result stays `readonly_region` (issue #16).
//! Rejecting everything gives the source back each time. The helpers serve the tests of simple fields and read-only content
//! controls too.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use crate::xml::NodeRef;
use stetdoc_testkit::fixtures::parts::{self, NS_W};
use stetdoc_testkit::fixtures::zip::Zip;

pub(super) fn wr(text: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{text}</w:t></w:r>"#)
}

/// A complex PAGE field whose result is `1`: begin, code, separate, result and end, each in a run of its own.
pub(super) fn complex() -> String {
    concat!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t xml:space="preserve">1</w:t></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )
    .to_string()
}

/// A document whose body is `body`, which checks clean.
pub(super) fn open(body: &str) -> Document {
    let xml = parts::Doc::new().body(body).bytes();
    let doc = Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap();
    assert!(doc.check().ok, "the input checks clean: {:?}", doc.check());
    doc
}

/// Runs `ops` on `doc`, tracked or not.
pub(super) fn edit(doc: &mut Document, ops: &str, tracked: bool) -> Result<Receipt, StetError> {
    let b = batch(doc, ops);
    let e = Edit::ops(b, stamp());
    let e = if tracked { e } else { e.untracked() };
    doc.edit(e)
}

pub(super) fn main_xml(doc: &Document) -> String {
    super::tests_edit_comment_tables::main_xml(doc)
}

/// The body as text with its structure: `{PAGE|1}` for a complex field (its code, then its result), `{=PAGE|1}` for a
/// `w:fldSimple`, `[` and `]` around a content control's content, ` / ` between paragraphs. Deleted text is left out.
pub(super) fn shape(doc: &Document) -> String {
    fn walk(n: NodeRef<'_>, out: &mut String) {
        for c in n.children() {
            if !c.is_element() {
                continue;
            }
            let text = || {
                c.children()
                    .map(|t| t.text().into_owned())
                    .collect::<String>()
            };
            match c.local_name() {
                "t" => out.push_str(&text()),
                "instrText" => out.push_str(text().trim()),
                "delText" | "delInstrText" | "pPr" | "rPr" | "sdtPr" | "sectPr" | "tblPr"
                | "tblGrid" | "trPr" | "tcPr" => {}
                "fldChar" => match c.attr(NS_W, "fldCharType").as_deref() {
                    Some("begin") => out.push('{'),
                    Some("separate") => out.push('|'),
                    _ => out.push('}'),
                },
                "fldSimple" => {
                    let instr = c.attr(NS_W, "instr").unwrap_or_default();
                    out.push_str(&format!("{{={}|", instr.trim()));
                    walk(c, out);
                    out.push('}');
                }
                "sdtContent" => {
                    out.push('[');
                    walk(c, out);
                    out.push(']');
                }
                "p" => {
                    if !out.is_empty() {
                        out.push_str(" / ");
                    }
                    walk(c, out);
                }
                _ => walk(c, out),
            }
        }
    }
    let view = doc.state.parts();
    let tree = view.tree(view.id("word/document.xml").unwrap()).unwrap();
    let body = tree.root().child_elements().next().unwrap();
    let mut out = String::new();
    walk(body, &mut out);
    out
}

/// The main part with adjacent plain runs of text merged: a tracked edit cuts runs where it starts and ends, and rejecting it
/// leaves them cut.
pub(super) fn merged(xml: &str) -> String {
    xml.replace(r#"</w:t></w:r><w:r><w:t xml:space="preserve">"#, "")
        .replace("</w:t></w:r><w:r><w:t>", "")
}

/// Applies `ops` tracked and accepts everything, tracked and rejects everything, and untracked. The accepted and the
/// untracked bodies have the shape `want`, the rejected main part is the source's up to the cut runs (`merged`), and each
/// document checks clean, the tracked one before it is reviewed too. Returns the tracked document before review.
pub(super) fn every_way(body: &str, ops: &str, want: &str) -> Document {
    let source = main_xml(&open(body));
    let mut tracked = open(body);
    edit(&mut tracked, ops, true).unwrap_or_else(|e| panic!("{ops} tracked: {e}"));
    assert!(tracked.check().ok, "{ops} tracked: {:?}", tracked.check());
    for review in ["accept", "reject"] {
        let mut doc = open(body);
        edit(&mut doc, ops, true).unwrap();
        run(&mut doc, &format!(r#"{{"op":"{review}","all":true}}"#));
        if review == "accept" {
            assert_eq!(shape(&doc), want, "{ops}, then accept: {}", main_xml(&doc));
        } else {
            assert_eq!(
                merged(&main_xml(&doc)),
                merged(&source),
                "{ops}, then reject"
            );
        }
        assert!(doc.check().ok, "{ops} then {review}: {:?}", doc.check());
    }
    let mut doc = open(body);
    edit(&mut doc, ops, false).unwrap_or_else(|e| panic!("{ops} untracked: {e}"));
    assert_eq!(shape(&doc), want, "{ops}, untracked: {}", main_xml(&doc));
    assert!(doc.check().ok, "{ops} untracked: {:?}", doc.check());
    tracked
}

/// The batch is refused as `readonly_region` in both modes, and the document is as it was.
pub(super) fn refused_both_ways(body: &str, ops: &str) {
    let before = open(body).save().unwrap().bytes;
    for tracked in [true, false] {
        let mut doc = open(body);
        let e = edit(&mut doc, ops, tracked)
            .err()
            .unwrap_or_else(|| panic!("{ops} (tracked: {tracked}) was applied"));
        assert_eq!(
            e.code(),
            ErrorCode::ReadonlyRegion,
            "{ops} (tracked: {tracked}): {e}"
        );
        assert_eq!(
            doc.save().unwrap().bytes,
            before,
            "{ops} (tracked: {tracked})"
        );
    }
}

/// `Alpha beta {PAGE|1} gamma.` and `Ends with {PAGE|1}`, the fixture of the issue.
fn fixture() -> String {
    format!(
        "<w:p>{}{}{}</w:p><w:p>{}{}</w:p>",
        wr("Alpha beta "),
        complex(),
        wr(" gamma."),
        wr("Ends with "),
        complex()
    )
}

#[test]
fn a_range_from_before_a_field_to_the_end_of_its_result_replaces_the_field() {
    let body = fixture();
    let doc = every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":6,"end":12,"with":"new text"}"#,
        "Alpha new text gamma. / Ends with {PAGE|1}",
    );
    // Tracked, the field is deleted whole and the new text is inserted after it.
    assert_eq!(final_text(&doc, "p1"), "Alpha new text gamma.");
    let xml = main_xml(&doc);
    assert!(
        xml.contains(r#"<w:delInstrText xml:space="preserve"> PAGE </w:delInstrText>"#),
        "{xml}"
    );
}

#[test]
fn replacing_all_the_text_of_a_paragraph_that_ends_with_a_field_replaces_the_field() {
    let body = fixture();
    let want = "Alpha beta {PAGE|1} gamma. / new text";
    every_way(
        &body,
        r#"{"op":"replace","ref":"p2","start":0,"end":11,"with":"new text"}"#,
        want,
    );
    every_way(
        &body,
        r#"{"op":"replace","ref":"p2","with":"new text"}"#,
        want,
    );
    // A field nested in another, at the end of the paragraph: the range holds both.
    let nested = format!(
        r#"<w:p>{}<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}{}<w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#,
        wr("Ends with "),
        wr("x"),
        complex()
    );
    every_way(
        &nested,
        r#"{"op":"replace","ref":"p1","with":"new text"}"#,
        "new text",
    );
}

#[test]
fn the_ranges_that_already_worked_still_do_and_exactly_the_result_stays_refused() {
    let body = fixture();
    refused_both_ways(
        &body,
        r#"{"op":"replace","ref":"p1","start":11,"end":12,"with":"new text"}"#,
    );
    refused_both_ways(
        &body,
        r#"{"op":"replace","ref":"p2","start":10,"end":11,"with":"new text"}"#,
    );
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":6,"end":13,"with":"new text"}"#,
        "Alpha new textgamma. / Ends with {PAGE|1}",
    );
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":11,"end":13,"with":"new text"}"#,
        "Alpha beta new textgamma. / Ends with {PAGE|1}",
    );
    every_way(
        &body,
        r#"{"op":"delete","ref":"p1","start":6,"end":12}"#,
        "Alpha  gamma. / Ends with {PAGE|1}",
    );
}

#[test]
fn the_rule_is_on_the_pieces_of_the_difference_not_on_the_range() {
    let body = fixture();
    // `beta 1` to `beta X` keeps `beta `, so the one piece is exactly the field's result.
    refused_both_ways(
        &body,
        r#"{"op":"replace","ref":"p1","start":6,"end":12,"with":"beta X"}"#,
    );
    // `beta 1` to `new text` is one piece that holds the field.
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":6,"end":12,"with":"new text"}"#,
        "Alpha new text gamma. / Ends with {PAGE|1}",
    );
}
