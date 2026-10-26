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

#[test]
fn a_move_whose_source_a_rejection_emptied_is_resolved_with_both_ranges() {
    // The moved-from text is an insertion inside the `w:moveFrom`: rejecting it empties the source, whose wrapper goes and
    // whose range markers stay. Accepting the move then resolves it whole (design `revisions.md`, 3.5), and the source's
    // markers go with the destination's instead of standing alone under the move's name.
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(concat!(
            r#"<w:p><w:r><w:t>Kept</w:t></w:r><w:moveToRangeStart w:id="2" w:author="A" w:date="2020-01-01T00:00:00Z" w:name="move1"/>"#,
            r#"<w:moveTo w:id="3" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t>moved</w:t></w:r></w:moveTo></w:p>"#,
            r#"<w:p><w:moveFromRangeStart w:id="5" w:author="A" w:date="2020-01-01T00:00:00Z" w:name="move1"/><w:moveToRangeEnd w:id="2"/>"#,
            r#"<w:moveFrom w:id="6" w:author="A" w:date="2020-01-01T00:00:00Z"><w:ins w:id="7" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t>b</w:t></w:r></w:ins></w:moveFrom>"#,
            r#"<w:moveFromRangeEnd w:id="5"/><w:r><w:t>Tail</w:t></w:r></w:p>"#,
        ))
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    let mut doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    let before = violations(&doc);
    let b = batch(
        &doc,
        r#"{"op":"reject","ids":[7]},{"op":"accept","all":true}"#,
    );
    doc.edit(crate::api::edit::Edit::ops(b, stamp()).untracked())
        .unwrap();
    assert_eq!(violations(&doc), before);
    assert_eq!(final_text(&doc, "p1"), "Keptmoved");
    assert!(
        doc.revisions(&RevisionFilter::default())
            .unwrap()
            .is_empty(),
        "nothing is pending"
    );
}
