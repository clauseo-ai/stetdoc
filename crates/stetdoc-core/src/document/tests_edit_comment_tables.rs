//! An untracked delete of a table at the edge of the body, through `Document::edit`: a comment whose range runs from inside the
//! table to a marker between blocks survives with an empty range (comments-and-notes.md 1.7), its markers stay in order, and
//! the marker between blocks keeps its XML.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::threads::tests_support::{Pkg, comment, ranged};

const START9: &str = "<w:commentRangeStart w:id=\"9\"/>";
const END9: &str = "<w:commentRangeEnd w:id=\"9\"/>";
const REF9: &str = "<w:r><w:commentReference w:id=\"9\"/></w:r>";

/// A one-cell table whose paragraph holds `inner`.
fn table(inner: &str) -> String {
    format!(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p>{inner}</w:p></w:tc></w:tr></w:tbl>"
    )
}

pub(super) fn main_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

pub(super) fn comments_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/comments.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

/// Deletes `t1` untracked and checks that comments 3 (inside the cell) and 9 both survive with their rows, with comment 9's start
/// before its end, and that the document checks clean. Returns the main part after.
fn delete_table_keeps_both(body: &str) -> String {
    let bytes = Pkg::new(body)
        .comments(&[
            comment(3, "Ann", &[None], "x"),
            comment(9, "Bob", &[None], "y"),
        ])
        .bytes();
    let mut doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    assert!(doc.check().ok, "the input checks clean: {:?}", doc.check());
    let b = batch(&doc, r#"{"op":"delete","ref":"t1"}"#);
    let receipt = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(
        receipt.comments.iter().all(|c| c.action != "removed"),
        "{:?}",
        receipt.comments
    );
    let xml = main_xml(&doc);
    assert!(!xml.contains("<w:tbl>"), "{xml}");
    let comments = comments_xml(&doc);
    for id in [3, 9] {
        assert!(
            comments.contains(&format!("w:id=\"{id}\"")),
            "c{id} keeps its row: {comments}"
        );
        assert_eq!(
            xml.matches(&format!("<w:commentRangeStart w:id=\"{id}\"/>"))
                .count(),
            1,
            "{xml}"
        );
        assert_eq!(
            xml.matches(&format!("<w:commentRangeEnd w:id=\"{id}\"/>"))
                .count(),
            1,
            "{xml}"
        );
        assert_eq!(
            xml.matches(&format!("<w:commentReference w:id=\"{id}\"/>"))
                .count(),
            1,
            "{xml}"
        );
    }
    assert!(xml.find(START9) < xml.find(END9), "start before end: {xml}");
    assert!(doc.check().ok, "{:?}", doc.check());
    xml
}

#[test]
fn deleting_a_first_table_keeps_a_comment_whose_range_ends_between_the_blocks() {
    // Comment035's layout: the start is in the table's first cell, the end between the table and the next paragraph, which
    // holds the reference. The end comes into that paragraph with what the table leaves, after it, so the start stays before
    // it, and its bytes are the same.
    let body = format!(
        "{}{END9}<w:p>{REF9}<w:r><w:t>after</w:t></w:r></w:p>",
        table(&format!("{START9}{}", ranged(3, "cell")))
    );
    let xml = delete_table_keeps_both(&body);
    assert!(
        xml.contains(&format!("{START9}<w:commentRangeStart w:id=\"3\"/>")),
        "{xml}"
    );
    assert!(
        xml.contains(&format!("{END9}{REF9}<w:r><w:t>after</w:t></w:r></w:p>")),
        "{xml}"
    );
}

#[test]
fn deleting_a_last_table_keeps_a_comment_whose_range_starts_between_the_blocks() {
    // The mirror: the start stands between the paragraph before and the table, the end and the reference are in the cell. What
    // the table leaves goes to the end of the paragraph before, and the start comes with it, before it.
    let body = format!(
        "<w:p><w:r><w:t>before</w:t></w:r></w:p>{START9}{}",
        table(&format!("{}{END9}{REF9}", ranged(3, "cell")))
    );
    let xml = delete_table_keeps_both(&body);
    assert!(
        xml.contains(&format!(
            "<w:r><w:t>before</w:t></w:r>{START9}<w:commentRangeStart w:id=\"3\"/>"
        )),
        "{xml}"
    );
}

#[test]
fn deleting_a_table_with_a_comment_on_all_of_it_inside_its_cells_keeps_it_as_before() {
    // The layout Word makes for a comment on a whole table: both markers and the reference in the cells. Nothing stands between
    // the blocks, so what the table leaves goes to the start of the next paragraph, as it did.
    let body = format!(
        "{}<w:p><w:r><w:t>after</w:t></w:r></w:p>",
        table(&format!("{START9}{}{END9}{REF9}", ranged(3, "cell")))
    );
    let xml = delete_table_keeps_both(&body);
    assert!(
        xml.contains(&format!("<w:p>{START9}<w:commentRangeStart w:id=\"3\"/>")),
        "{xml}"
    );
    assert!(
        xml.contains(&format!("{END9}{REF9}<w:r><w:t>after</w:t></w:r></w:p>")),
        "{xml}"
    );
}
