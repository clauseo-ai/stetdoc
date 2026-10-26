//! Rejection through `Document::edit`, on documents that already have a problem the rejection must give back as it was.

use super::tests_edit::*;
use super::*;
use crate::api::check::{Class, FindingCode};

/// The codes of the violations a document has, each once, sorted.
fn violations(doc: &Document) -> Vec<FindingCode> {
    let mut found: Vec<FindingCode> = doc
        .check()
        .by_code
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| c.class() == Class::Violation)
        .collect();
    found.sort();
    found
}

#[test]
fn rejecting_a_paragraph_inserted_at_the_end_gives_back_an_attribute_the_schema_does_not_know() {
    // Word 2007 betas wrote `paraId` in a namespace the schema does not know, so every paragraph already has an unknown
    // attribute. A paragraph inserted after the last one, which is empty, keeps the original's element with its attributes
    // and an inserted mark; rejecting it removes that element and gives its attributes to the paragraph that stays, which
    // came from the original and carries its problem.
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .root_attrs(r#" xmlns:w14="http://schemas.microsoft.com/office/word/2007/5/30/wordml""#)
        .body(concat!(
            r#"<w:p w14:paraId="71e7e559"><w:r><w:t>First</w:t></w:r></w:p>"#,
            r#"<w:p w14:paraId="6bfb5e17" w:rsidR="00000000"/>"#,
        ))
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    let mut doc = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
    let before = violations(&doc);
    assert!(
        before.contains(&FindingCode::SchemaUnknownAttribute),
        "{before:?}"
    );
    run(
        &mut doc,
        r#"{"op":"insert","ref":"p2","place":"after","paragraphs":["terminate liability clause"]}"#,
    );
    run(&mut doc, r#"{"op":"reject","author":"Clauseo"}"#);
    assert_eq!(violations(&doc), before);
    let original = Document::open(bytes, &OpenOptions::default()).unwrap();
    assert_eq!(final_text(&doc, "p1"), final_text(&original, "p1"));
    // The paragraph that stays is the empty one the original had, under the ref of the inserted paragraph.
    assert_eq!(final_text(&doc, "p3"), "");
}
