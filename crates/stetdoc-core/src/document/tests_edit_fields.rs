//! Fields that move between paragraphs when an edit joins two of them: the view straight after the edit is the view a resume of
//! the same bytes gives (law L4). A join moves the content of one paragraph into another, and the refresh of the story looks at
//! the paragraphs touched since the story was last built, so the join must touch the one that took the content even when the
//! operation touched it before (`revisions/join.rs`); a refresh that does not look at it again drops the fields that came in.

use super::tests_edit::*;
use super::*;
use stetdoc_testkit::fixtures::parts::{self, DECL, NS_W};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

/// A complex PAGE field: begin, instrText, separate, its result, end, each in a run of its own.
const COMPLEX: &str = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;

/// A simple NUMPAGES field.
const SIMPLE: &str = r#"<w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>2</w:t></w:r></w:fldSimple>"#;

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn run_of(t: &str) -> String {
    format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#)
}

/// The inner XML of a paragraph that holds both fields after some text: `Smaller {PAGE} of {NUMPAGES}`.
fn fielded() -> String {
    format!("{}{COMPLEX}{}{SIMPLE}", run_of("Smaller "), run_of(" of "))
}

fn para(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

/// A document whose body is `p1` and `p2`, then a table of one cell that holds `p3` and `p4`, then `p5` with a footnote
/// reference; the default header holds `p6` and `p7`, and the footnote `p8` and `p9`. Only the paragraph `fields_in` (one of
/// p1, p3, p6, p8) holds the fields, and the others of those four hold plain text: a story whose index holds no other field shows
/// a stale index plainly, where another paragraph's fields of the same kind would stand in for the lost ones in the view.
fn document(fields_in: &str) -> Document {
    let at = |token: &str| {
        if token == fields_in {
            fielded()
        } else {
            run_of("Plain")
        }
    };
    let note_ref = r#"<w:r><w:footnoteReference w:id="1"/></w:r>"#;
    let body = [
        para(&at("p1")),
        para(&run_of("After")),
        format!(
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc><w:tcPr/>{}{}</w:tc></w:tr></w:tbl>",
            para(&at("p3")),
            para(&run_of("Cell after"))
        ),
        // The section that ends here names the header.
        para(&format!(
            r#"<w:pPr><w:sectPr><w:headerReference w:type="default" r:id="rId2"/></w:sectPr></w:pPr>{}{note_ref}"#,
            run_of("Noted")
        )),
    ]
    .concat();
    let doc = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:r="{NS_R}""#))
        .body(&body)
        .bytes();
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/><Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/></Types>",
    );
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{NS_R}/footnotes\" Target=\"footnotes.xml\"/><Relationship Id=\"rId2\" Type=\"{NS_R}/header\" Target=\"header1.xml\"/></Relationships>"
    );
    let notes = format!(
        "{DECL}<w:footnotes xmlns:w=\"{NS_W}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id=\"1\">{}{}</w:footnote></w:footnotes>",
        para(&at("p8")),
        para(&run_of("Note after"))
    );
    let header = format!(
        "{DECL}<w:hdr xmlns:w=\"{NS_W}\">{}{}</w:hdr>",
        para(&at("p6")),
        para(&run_of("Header after"))
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new("word/footnotes.xml", notes));
    entries.push(Entry::new("word/header1.xml", header));
    Document::open(Zip::new(entries).build(), &OpenOptions::default()).unwrap()
}

/// The readable file of the working copy.
fn readable(doc: &Document) -> String {
    let wc = doc.export().unwrap();
    wc.files
        .iter()
        .map(|(name, text)| format!("== {name}\n{text}"))
        .collect()
}

/// Runs the operations tracked, in one batch, and checks that the export equals the export of a document resumed from it, and
/// that the line of the paragraph `token` in the readable file and the receipt both show the two fields.
fn joins_back(ops: &str, token: &str) {
    joins_back_after(None, ops, token);
}

/// `joins_back`, after a batch `first` that is not checked.
fn joins_back_after(first: Option<&str>, ops: &str, token: &str) {
    checked(document(token), first, ops, token);
}

/// `joins_back_after` on `doc`.
fn checked(mut doc: Document, first: Option<&str>, ops: &str, token: &str) {
    if let Some(first) = first {
        run(&mut doc, first);
    }
    let receipt = run(&mut doc, ops);
    let wc = doc.export().unwrap();
    let back = Document::resume(&wc, &OpenOptions::default()).unwrap();
    assert_eq!(
        readable(&doc),
        readable(&back),
        "the readable files after {ops} differ from a resume's"
    );
    assert_eq!(back.export().unwrap(), wc, "{ops}");
    let text = readable(&doc);
    let line = text
        .lines()
        .find(|l| l.starts_with(&format!("{token} ")))
        .unwrap_or_else(|| panic!("no line for {token} after {ops}:\n{text}"));
    for field in ["{field PAGE \"1\"}", "{field NUMPAGES \"2\"}"] {
        assert!(line.contains(field), "{field} after {ops}: {line}");
        // A receipt that shows the paragraph (a delete or a merge does; a rejection by author lists no text) shows its fields.
        assert!(
            !receipt.text.contains("maller") || receipt.text.contains(field),
            "{field} after {ops}: {}",
            receipt.text
        );
    }
}

#[test]
fn a_tracked_split_then_delete_keeps_the_fields_that_join_back() {
    // The nightly case: the split leaves the first half "S" ending in the author's own inserted mark, and the delete takes "S"
    // and that mark, joining the second half, which holds the fields, back into the first.
    for at in [1, 3] {
        joins_back(
            &format!(r#"{{"op":"split","ref":"p1","at":{at}}},{{"op":"delete","ref":"p1"}}"#),
            "p1",
        );
    }
}

#[test]
fn the_same_join_in_a_cell_a_header_and_a_note_keeps_its_fields() {
    // p3 is in a table cell, p6 in the header, p8 in the footnote: the join is the same code in every story.
    for token in ["p3", "p6", "p8"] {
        joins_back(
            &format!(
                r#"{{"op":"split","ref":"{token}","at":1}},{{"op":"delete","ref":"{token}"}}"#
            ),
            token,
        );
    }
}

#[test]
fn other_joins_of_the_authors_own_mark_keep_their_fields() {
    // A merge of the first half, whose mark is the author's own insertion, and a rejection of the split, which takes that mark
    // out and joins the halves; and the same merge in the cell, the header and the note.
    for token in ["p1", "p3", "p6", "p8"] {
        joins_back(
            &format!(r#"{{"op":"split","ref":"{token}","at":1}},{{"op":"merge","ref":"{token}"}}"#),
            token,
        );
        // The rejection, and the delete, in a batch after the split's.
        let split = format!(r#"{{"op":"split","ref":"{token}","at":1}}"#);
        joins_back_after(Some(&split), r#"{"op":"reject","author":"Clauseo"}"#, token);
        joins_back_after(
            Some(&split),
            &format!(r#"{{"op":"delete","ref":"{token}"}}"#),
            token,
        );
    }
}
