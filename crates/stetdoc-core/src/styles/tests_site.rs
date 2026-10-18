//! The reads that build a `Site`: what a tracked change replaced, the numbering a paragraph removes,
//! the shading of the cell around a paragraph, and the page colour.

use super::site::{cell_fill, direct_paragraph, direct_run, numbering_removed, page_background};
use super::tests_support::*;
use crate::wordml::Paragraph;
use crate::wordml::testing::parse;

const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";

fn paragraph_of(body: &str) -> (crate::wordml::SpanTree, crate::wordml::NodeId) {
    let t = tree(body);
    let id = first(t.root(), "p").map(|n| n.id()).unwrap();
    (t, id)
}

const CHANGED: &str = "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:jc w:val=\"center\"/>\
    <w:rPr><w:b/><w:rPrChange w:id=\"1\"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr>\
    <w:pPrChange w:id=\"2\"><w:pPr><w:pStyle w:val=\"Body\"/><w:jc w:val=\"left\"/></w:pPr></w:pPrChange>\
    </w:pPr><w:r><w:t>x</w:t></w:r></w:p>";

#[test]
fn a_paragraph_is_read_as_it_is_now_except_in_original() {
    // Final and Redline read the properties as written. Original reads what the changes replaced: the old
    // style, the old direct properties and the old paragraph mark properties.
    let (t, id) = paragraph_of(CHANGED);
    let p = Paragraph::new(&t, id).unwrap();
    let now = direct_paragraph(&p, false);
    assert_eq!(now.style.as_deref(), Some("Heading1"));
    assert_eq!(now.ppr.and_then(|p| p.val("jc")).as_deref(), Some("center"));
    assert_eq!(now.mark.and_then(|m| m.on_off("b")), Some(true));
    let old = direct_paragraph(&p, true);
    assert_eq!(old.style.as_deref(), Some("Body"));
    assert_eq!(old.ppr.and_then(|p| p.val("jc")).as_deref(), Some("left"));
    assert_eq!(old.mark.and_then(|m| m.on_off("b")), None);
    assert_eq!(old.mark.and_then(|m| m.on_off("i")), Some(true));
}

#[test]
fn an_old_snapshot_with_no_style_means_the_default_style() {
    let (t, id) = paragraph_of(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:pPrChange w:id=\"2\"><w:pPr/></w:pPrChange>\
         </w:pPr></w:p>",
    );
    let p = Paragraph::new(&t, id).unwrap();
    assert_eq!(
        direct_paragraph(&p, false).style.as_deref(),
        Some("Heading1")
    );
    assert_eq!(direct_paragraph(&p, true).style, None);
}

#[test]
fn a_paragraph_without_a_change_reads_the_same_in_every_mode() {
    let (t, id) =
        paragraph_of("<w:p><w:pPr><w:pStyle w:val=\"Body\"/><w:rPr><w:b/></w:rPr></w:pPr></w:p>");
    let p = Paragraph::new(&t, id).unwrap();
    for original in [false, true] {
        let d = direct_paragraph(&p, original);
        assert_eq!(d.style.as_deref(), Some("Body"));
        assert_eq!(d.mark.and_then(|m| m.on_off("b")), Some(true));
    }
    let (t, id) = paragraph_of("<w:p/>");
    let bare = direct_paragraph(&Paragraph::new(&t, id).unwrap(), true);
    assert!(bare.style.is_none() && bare.ppr.is_none() && bare.mark.is_none());
}

#[test]
fn a_run_is_read_as_it_is_now_except_in_original() {
    let t = tree(
        "<w:r><w:rPr><w:rStyle w:val=\"Strong\"/><w:b/><w:rPrChange w:id=\"3\"><w:rPr>\
         <w:rStyle w:val=\"Emphasis\"/></w:rPr></w:rPrChange></w:rPr></w:r>",
    );
    let rpr = props(&t, "rPr");
    let now = direct_run(rpr, false).unwrap();
    assert_eq!(now.val("rStyle").as_deref(), Some("Strong"));
    assert_eq!(now.on_off("b"), Some(true));
    let old = direct_run(rpr, true).unwrap();
    assert_eq!(old.val("rStyle").as_deref(), Some("Emphasis"));
    assert_eq!(old.on_off("b"), None);
    assert!(direct_run(None, true).is_none());
    // A run that was plain before has an empty snapshot, which is not the same as no properties now.
    let t = tree("<w:r><w:rPr><w:b/><w:rPrChange w:id=\"3\"><w:rPr/></w:rPrChange></w:rPr></w:r>");
    let old = direct_run(props(&t, "rPr"), true).unwrap();
    assert_eq!(old.on_off("b"), None);
}

#[test]
fn only_a_num_id_of_zero_removes_numbering() {
    let removed = |ppr: &str| {
        let t = tree(&format!("<w:p><w:pPr>{ppr}</w:pPr></w:p>"));
        numbering_removed(props(&t, "pPr"))
    };
    assert!(removed(
        "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"0\"/></w:numPr>"
    ));
    assert!(!removed("<w:numPr><w:numId w:val=\"4\"/></w:numPr>"));
    assert!(!removed("<w:numPr><w:ilvl w:val=\"1\"/></w:numPr>"));
    assert!(!removed("<w:jc w:val=\"left\"/>"));
    assert!(!numbering_removed(None));
}

fn fill_of(body: &str) -> Option<String> {
    let t = tree(body);
    let p = first(t.root(), "p").unwrap();
    cell_fill(p).map(|f| f.into_owned())
}

#[test]
fn the_fill_is_that_of_the_cell_around_the_paragraph() {
    let cell =
        |pr: &str, inner: &str| format!("<w:tbl><w:tr><w:tc>{pr}{inner}</w:tc></w:tr></w:tbl>");
    let shd = |fill: &str| format!("<w:tcPr><w:shd w:val=\"clear\" w:fill=\"{fill}\"/></w:tcPr>");
    let para = "<w:p><w:r><w:t>x</w:t></w:r></w:p>";
    assert_eq!(
        fill_of(&cell(&shd("1F3864"), para)).as_deref(),
        Some("1F3864")
    );
    assert_eq!(fill_of(&cell("<w:tcPr/>", para)), None);
    assert_eq!(fill_of(&cell("", para)), None);
    assert_eq!(fill_of(para), None, "outside a table");
    // In a content control inside the cell the paragraph is still in the cell.
    let wrapped = format!("<w:sdt><w:sdtContent>{para}</w:sdtContent></w:sdt>");
    assert_eq!(
        fill_of(&cell(&shd("FF0000"), &wrapped)).as_deref(),
        Some("FF0000")
    );
    // A nested table: the innermost cell answers, even when it has no shading of its own.
    let nested = cell(&shd("FF0000"), &cell("", para));
    assert_eq!(fill_of(&nested), None);
    let nested = cell(&shd("FF0000"), &cell(&shd("00FF00"), para));
    assert_eq!(fill_of(&nested).as_deref(), Some("00FF00"));
}

#[test]
fn the_page_colour_is_the_documents_background() {
    let doc = |inner: &str| {
        parse(
            "word/document.xml",
            &format!("<w:document {W}>{inner}<w:body/></w:document>"),
        )
    };
    let coloured = doc("<w:background w:color=\"FFFF00\"/>");
    assert_eq!(page_background(&coloured).as_deref(), Some("FFFF00"));
    assert_eq!(page_background(&doc("")), None);
    assert_eq!(page_background(&doc("<w:background/>")), None);
}
