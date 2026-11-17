//! Deleting all of an inline content control's or smart tag's content, through `Document::edit`: a field, simple or
//! complex, goes the way text of the same extent does. Untracked, or tracked and then accepted, the control stays with no
//! content, as Word keeps a control whose content is deleted. Rejecting gives the field back inside the control.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

const PLAIN: &str = r#"<w:r><w:t>1,000</w:t></w:r>"#;
const SIMPLE: &str =
    r#"<w:fldSimple w:instr=" DOCPROPERTY Amount "><w:r><w:t>1,000</w:t></w:r></w:fldSimple>"#;
const COMPLEX: &str = concat!(
    r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#,
    r#"<w:r><w:instrText xml:space="preserve"> DOCPROPERTY Amount </w:instrText></w:r>"#,
    r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1,000</w:t></w:r>"#,
    r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
);

fn sdt(inner: &str) -> String {
    format!(
        r#"<w:sdt><w:sdtPr><w:alias w:val="Amt"/><w:tag w:val="amt"/></w:sdtPr><w:sdtContent>{inner}</w:sdtContent></w:sdt>"#
    )
}

fn smart_tag(inner: &str) -> String {
    format!(r#"<w:smartTag w:uri="urn:x" w:element="amount">{inner}</w:smartTag>"#)
}

fn package(inner: &str) -> Vec<u8> {
    let body = format!(
        r#"<w:p><w:r><w:t>Intro</w:t></w:r></w:p><w:p><w:r><w:t xml:space="preserve">Pay </w:t></w:r>{inner}<w:r><w:t xml:space="preserve"> now.</w:t></w:r></w:p>"#
    );
    let xml = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &OpenOptions::default()).unwrap()
}

/// The XML of the paragraph that starts with "Pay".
fn paragraph(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let xml = String::from_utf8(view.tree(id).unwrap().serialize()).unwrap();
    let start = xml.find("Pay ").unwrap();
    let start = xml[..start].rfind("<w:p>").unwrap();
    let end = start + xml[start..].find("</w:p>").unwrap();
    xml[start..end].to_owned()
}

/// Deletes "1,000" untracked, and tracked then accepted, and returns both paragraphs; rejecting gives the source back.
fn deleted(inner: &str) -> (String, String) {
    let source = package(inner);
    let op = r#"{"op":"delete","ref":"p2","find":"1,000"}"#;
    let mut untracked = open(&source);
    let b = batch(&untracked, op);
    untracked.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    let mut tracked = open(&source);
    run(&mut tracked, op);
    let mut rejected = open(&tracked.save().unwrap().bytes);
    run(&mut tracked, r#"{"op":"accept","all":true}"#);
    run(&mut rejected, r#"{"op":"reject","all":true}"#);
    assert_eq!(
        final_text(&rejected, "p2"),
        final_text(&open(&source), "p2")
    );
    for doc in [&untracked, &tracked, &rejected] {
        assert!(doc.check().ok, "{:?}", doc.check());
    }
    (paragraph(&untracked), paragraph(&tracked))
}

#[test]
fn a_field_deleted_in_a_content_control_leaves_it_empty_as_text_does() {
    let empty = "<w:sdtContent></w:sdtContent></w:sdt>";
    let (text_untracked, text_accepted) = deleted(&sdt(PLAIN));
    assert!(text_untracked.contains(empty), "{text_untracked}");
    assert_eq!(text_untracked, text_accepted);
    for field in [SIMPLE, COMPLEX] {
        let (untracked, accepted) = deleted(&sdt(field));
        assert_eq!(untracked, text_untracked, "{field}");
        assert_eq!(accepted, text_accepted, "{field}");
    }
}

#[test]
fn a_field_deleted_in_a_smart_tag_leaves_it_empty_as_text_does() {
    let empty = r#"<w:smartTag w:uri="urn:x" w:element="amount"></w:smartTag>"#;
    let (text_untracked, text_accepted) = deleted(&smart_tag(PLAIN));
    assert!(text_untracked.contains(empty), "{text_untracked}");
    assert_eq!(text_untracked, text_accepted);
    for field in [SIMPLE, COMPLEX] {
        let (untracked, accepted) = deleted(&smart_tag(field));
        assert_eq!(untracked, text_untracked, "{field}");
        assert_eq!(accepted, text_accepted, "{field}");
    }
}
