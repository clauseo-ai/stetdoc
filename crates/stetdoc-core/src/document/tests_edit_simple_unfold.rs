//! A simple field that a tracked deletion writes as a complex field, when other authors' revisions are in its result (issue
//! #24): the copies of another author's format change on the field's runs are one change with it, and a result inserted by
//! two changes keeps its `w:fldSimple`, so that the field goes or stays as the source's does.

use super::tests_edit::*;
use super::tests_edit_field_ranges::{edit, main_xml, open, shape, wr};
use super::*;
use crate::api::revision::ChangeKind;

fn simple_with(inner: &str) -> String {
    format!(r#"<w:fldSimple w:instr=" PAGE ">{inner}</w:fldSimple>"#)
}

fn by(author: &str, id: u32, date: &str) -> String {
    format!(r#"w:id="{id}" w:author="{author}" w:date="{date}""#)
}

/// The run format changes of `author`, each as the ids it acts on.
fn formats_of(doc: &Document, author: &str) -> Vec<Vec<i64>> {
    doc.revisions(&RevisionFilter::default())
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == ChangeKind::RunFormat && r.author.as_deref() == Some(author))
        .map(|r| r.ids)
        .collect()
}

#[test]
fn the_copies_of_another_authors_format_change_are_one_change_with_it() {
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        wr("See page "),
        simple_with(&format!(
            r#"<w:r><w:rPr><w:i/><w:rPrChange {}><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr><w:t>1</w:t></w:r>"#,
            by("Other", 90, "2026-01-01T00:00:00Z")
        )),
        wr(" for details.")
    );
    let ops = r#"{"op":"delete","ref":"p1","start":9,"end":10}"#;
    assert_eq!(formats_of(&open(&body), "Other").len(), 1);
    let mut doc = open(&body);
    edit(&mut doc, ops, true).unwrap();
    // One change, as in the source, which acts on the result's change and its four copies.
    let formats = formats_of(&doc, "Other");
    assert_eq!(formats.len(), 1, "{formats:?}");
    assert_eq!(formats[0].len(), 5, "{formats:?}");
    assert!(formats[0].contains(&90), "{formats:?}");
    // Rejecting it by its id rejects the copies too: then rejecting the deletion and accepting the rest is bold everywhere.
    run(&mut doc, r#"{"op":"reject","ids":[90]}"#);
    run(&mut doc, r#"{"op":"reject","author":"Clauseo"}"#);
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    let xml = main_xml(&doc);
    assert!(!xml.contains("<w:i/>"), "{xml}");
    assert_eq!(xml.matches("<w:b/>").count(), 5, "{xml}");
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn a_result_inserted_by_two_changes_keeps_its_simple_field() {
    // `1` inserted by Ann and `2` by Bob: rejecting Ann alone keeps the field with `2` in the source, so the deletion leaves
    // the `w:fldSimple` (with its runs deleted inside it) rather than give each a marker that the other's rejection would lose.
    let date = "2026-01-01T00:00:00Z";
    let two = format!(
        "<w:p>{}{}{}</w:p>",
        wr("See "),
        simple_with(&format!(
            "<w:ins {}>{}</w:ins><w:ins {}>{}</w:ins>",
            by("Ann", 90, date),
            wr("1"),
            by("Bob", 91, date),
            wr("2")
        )),
        wr(" tail")
    );
    let ops = r#"{"op":"delete","ref":"p1","start":4,"end":6}"#;
    let mut doc = open(&two);
    edit(&mut doc, ops, true).unwrap();
    assert!(
        main_xml(&doc).contains("<w:fldSimple"),
        "{}",
        main_xml(&doc)
    );
    assert!(doc.check().ok, "{:?}", doc.check());
    run(&mut doc, r#"{"op":"reject","author":"Clauseo"}"#);
    run(&mut doc, r#"{"op":"reject","author":"Ann"}"#);
    let mut source = open(&two);
    run(&mut source, r#"{"op":"reject","author":"Ann"}"#);
    assert_eq!(shape(&doc), shape(&source));
    assert_eq!(shape(&doc), "See {=PAGE|2} tail");
    // Two insertions of one change are one decision: the field is written as a complex field.
    let one = two.replace(r#"w:author="Bob""#, r#"w:author="Ann""#);
    let mut doc = open(&one);
    edit(&mut doc, ops, true).unwrap();
    assert!(
        !main_xml(&doc).contains("<w:fldSimple"),
        "{}",
        main_xml(&doc)
    );
    assert!(doc.check().ok, "{:?}", doc.check());
}
