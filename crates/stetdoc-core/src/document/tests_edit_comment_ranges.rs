//! A delete from the middle of one paragraph to the middle of another, through `Document::edit`, across a comment whose range
//! starts or ends in a paragraph that goes whole or at a marker between blocks: the comment survives (comments-and-notes.md
//! 1.7), with an empty range if all of its text went, and its markers stay in order, untracked, and tracked and then accepted
//! or rejected.

use super::tests_edit::*;
use super::tests_edit_comment_tables::{comments_xml, main_xml};
use super::*;
use crate::api::edit::Edit;
use crate::threads::tests_support::{Pkg, comment};

const START9: &str = "<w:commentRangeStart w:id=\"9\"/>";
const END9: &str = "<w:commentRangeEnd w:id=\"9\"/>";
const REF9: &str = "<w:r><w:commentReference w:id=\"9\"/></w:r>";

fn para(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

fn wr(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn open(body: &str) -> Document {
    let bytes = Pkg::new(body)
        .comments(&[comment(9, "Bob", &[None], "y")])
        .bytes();
    let doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    assert!(doc.check().ok, "the input checks clean: {:?}", doc.check());
    doc
}

/// Checks that comment 9 is whole: its row, one start, one end and one reference, the start before the end and the end before
/// the reference, and that the document checks clean. Returns the main part.
fn keeps_c9(doc: &Document) -> String {
    let xml = main_xml(doc);
    assert!(
        comments_xml(doc).contains("w:id=\"9\""),
        "c9 keeps its row: {}",
        comments_xml(doc)
    );
    for m in [START9, END9, "<w:commentReference w:id=\"9\"/>"] {
        assert_eq!(xml.matches(m).count(), 1, "one {m}: {xml}");
    }
    let (s, e, r) = (xml.find(START9), xml.find(END9), xml.find(REF9));
    assert!(s < e && e < r, "start, end, reference in order: {xml}");
    assert!(doc.check().ok, "{:?}", doc.check());
    xml
}

/// Applies `op` untracked and checks c9. Returns the main part and the final text of `p1`.
fn untracked(body: &str, op: &str) -> (String, String) {
    let mut doc = open(body);
    let b = batch(&doc, op);
    let receipt = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(
        receipt.comments.iter().all(|c| c.action != "removed"),
        "{:?}",
        receipt.comments
    );
    let xml = keeps_c9(&doc);
    (xml, final_text(&doc, "p1"))
}

/// Applies `op` tracked, then accepts or rejects everything, checking c9 after each. Returns the main part after the review.
fn tracked_then(body: &str, op: &str, review: &str) -> String {
    let mut doc = open(body);
    super::tests_edit::run(&mut doc, op);
    keeps_c9(&doc);
    super::tests_edit::run(&mut doc, &format!(r#"{{"op":"{review}","all":true}}"#));
    keeps_c9(&doc)
}

/// Untracked, tracked and accepted, tracked and rejected: c9 is whole each time. Returns the untracked main part and `p1`.
fn every_way(body: &str, op: &str) -> (String, String) {
    for review in ["accept", "reject"] {
        tracked_then(body, op, review);
    }
    untracked(body, op)
}

#[test]
fn a_range_that_starts_in_a_paragraph_deleted_whole_keeps_its_start_before_its_end() {
    // The smallest form of the nightly failure: the start is at the head of the middle paragraph, which goes whole; the end and
    // the reference are in the last, after the deleted text. The start comes into the joined paragraph where the text went.
    let body = format!(
        "{}{}{}",
        para(&wr("keep this")),
        para(&format!("{START9}{}", wr("gone"))),
        para(&format!("{}{}{END9}{REF9}", wr("half "), wr("kept"))),
    );
    let op = r#"{"op":"delete","ref":"p1","to":"p3","start":4,"end":5}"#;
    let (xml, p1) = every_way(&body, op);
    assert_eq!(p1, "keepkept");
    let kept = xml.find(">kept<").unwrap();
    assert!(xml.find(START9).unwrap() < kept, "{xml}");
    assert!(kept < xml.find(END9).unwrap(), "{xml}");
}

#[test]
fn a_range_wholly_inside_the_deleted_text_becomes_an_empty_range_where_the_text_was() {
    // resume-styling.docx's layout: the start at the head of one middle paragraph, the end and the reference at the tail of the
    // next, and the delete runs from the paragraph before them to the one after. The comment stays, empty, at the cut.
    let body = format!(
        "{}{}{}{}",
        para(&wr("keep this")),
        para(&format!("{START9}{}", wr("gone"))),
        para(&format!("{}{END9}{REF9}", wr("also gone"))),
        para(&wr("cut tail")),
    );
    let op = r#"{"op":"delete","ref":"p1","to":"p4","start":4,"end":3}"#;
    let (xml, p1) = every_way(&body, op);
    assert_eq!(p1, "keep tail");
    assert!(
        xml.contains(&format!("{START9}{END9}{REF9}")),
        "an empty range: {xml}"
    );
}

#[test]
fn a_range_that_starts_between_the_blocks_before_a_deleted_paragraph_keeps_its_start_at_the_cut() {
    // The mirror of Comment035's layout in a text range: the start stands between the first paragraph and the one that goes
    // whole, and the end is in the last, after the deleted text.
    let body = format!(
        "{}{START9}{}{}",
        para(&wr("keep this")),
        para(&wr("gone")),
        para(&format!("{}{END9}{REF9}", wr("cut tail"))),
    );
    let op = r#"{"op":"delete","ref":"p1","to":"p3","start":4,"end":3}"#;
    let (xml, p1) = every_way(&body, op);
    assert_eq!(p1, "keep tail");
    let tail = xml.find("> tail<").unwrap();
    assert!(xml.find(START9).unwrap() < tail, "{xml}");
}

#[test]
fn a_range_that_ends_between_the_blocks_after_a_deleted_paragraph_keeps_its_end_at_the_cut() {
    // The reverse, and Comment035's layout: the start is in the first paragraph, before the deleted text, and the end stands
    // between the paragraph that goes whole and the last, which holds the reference. The end stays before the text after the
    // cut, so the range does not grow over it.
    let body = format!(
        "{}{}{END9}{}",
        para(&format!("{}{START9}{}", wr("keep "), wr("this"))),
        para(&wr("gone")),
        para(&format!("{REF9}{}", wr("cut tail"))),
    );
    let op = r#"{"op":"delete","ref":"p1","to":"p3","start":7,"end":3}"#;
    let (xml, p1) = every_way(&body, op);
    assert_eq!(p1, "keep th tail");
    assert!(xml.contains(&format!("{END9}{REF9}")), "{xml}");
    assert!(xml.find(END9) < xml.find("> tail<"), "{xml}");
}

#[test]
fn a_range_that_ends_with_its_reference_in_a_deleted_paragraph_keeps_its_order_as_before() {
    // The reverse in the layout Word makes, the reference right after the end, in the paragraph that goes whole: what that
    // paragraph leaves goes to the start of the last, as it did.
    let body = format!(
        "{}{}{}",
        para(&format!("{}{START9}{}", wr("keep "), wr("this"))),
        para(&format!("{}{END9}{REF9}", wr("gone"))),
        para(&wr("cut tail")),
    );
    let op = r#"{"op":"delete","ref":"p1","to":"p3","start":7,"end":3}"#;
    let (xml, p1) = every_way(&body, op);
    assert_eq!(p1, "keep th tail");
    assert!(xml.contains(&format!("{END9}{REF9}")), "{xml}");
    assert!(xml.find(END9) < xml.find("> tail<"), "{xml}");
}
