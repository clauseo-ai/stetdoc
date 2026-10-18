//! The census: ids that must be unique and ranges that must be paired, one fixture each, and the
//! cache that holds a census per part generation.

use super::census::{Census, revision_findings};
use super::found::Found;
use super::keys::Key;
use super::tests::{document_xml, tree};
use crate::api::check::FindingCode as C;
use std::sync::Arc;

const REV: &str = r#"w:author="a" w:date="2024-01-01T00:00:00Z""#;

fn census(body: &str) -> Census {
    Census::of(&tree(&document_xml(body)))
}

/// The code and subject of each finding of one part, in order.
fn part(body: &str) -> Vec<(C, String)> {
    read(&census(body).part_findings("word/document.xml"))
}

fn read(found: &[Found]) -> Vec<(C, String)> {
    found
        .iter()
        .map(|f| match &f.key {
            Key::Rule { code, subject } => (*code, subject.clone()),
            Key::Schema(k) => panic!("a schema key in the census: {k:?}"),
        })
        .collect()
}

fn expected(list: &[(C, &str)]) -> Vec<(C, String)> {
    list.iter().map(|(c, s)| (*c, (*s).to_string())).collect()
}

/// The revision-id findings over bodies that are parts of one document.
fn revisions(bodies: &[&str]) -> Vec<(C, String)> {
    let all: Vec<Census> = bodies.iter().map(|b| census(b)).collect();
    let refs: Vec<&Census> = all.iter().collect();
    read(&revision_findings(&refs))
}

fn ins(id: u32) -> String {
    format!(r#"<w:p><w:ins w:id="{id}" {REV}><w:r><w:t>x</w:t></w:r></w:ins></w:p>"#)
}

#[test]
fn a_revision_id_is_unique_across_the_parts_of_a_document() {
    let (one, seven) = (ins(1), ins(7));
    assert_eq!(revisions(&[&one, &seven]), expected(&[]));
    assert_eq!(
        revisions(&[&one, &one]),
        expected(&[(C::RevisionIdDuplicate, "1")])
    );
    let twice = format!("{one}{one}{seven}{seven}{seven}");
    assert_eq!(
        revisions(&[&twice]),
        expected(&[(C::RevisionIdDuplicate, "1"), (C::RevisionIdDuplicate, "7")]),
        "once for an id, however many times it is used"
    );
}

#[test]
fn every_element_that_carries_a_revision_id_is_counted() {
    for element in [
        format!(r#"<w:p><w:pPr><w:rPr><w:ins w:id="5" {REV}/></w:rPr></w:pPr></w:p>"#),
        format!(r#"<w:p><w:pPr><w:pPrChange w:id="5" {REV}><w:pPr/></w:pPrChange></w:pPr></w:p>"#),
        format!(
            r#"<w:p><w:moveFromRangeStart w:id="5" w:name="m" {REV}/><w:moveFromRangeEnd w:id="5"/></w:p>"#
        ),
        format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:trPr><w:del w:id="5" {REV}/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#
        ),
        format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tc><w:tcPr><w:cellIns w:id="5" {REV}/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>"#
        ),
    ] {
        assert_eq!(
            revisions(&[&element, &ins(5)]),
            expected(&[(C::RevisionIdDuplicate, "5")]),
            "{element}"
        );
    }
}

#[test]
fn what_word_does_not_read_has_no_revision_id() {
    let one = ins(1);
    // the old properties inside a property change are a snapshot of what was there, with ids that
    // are not the document's
    let snapshot = format!(
        r#"<w:p><w:pPr><w:rPr><w:ins w:id="1" {REV}/></w:rPr><w:pPrChange w:id="2" {REV}><w:pPr><w:rPr><w:ins w:id="1" {REV}/></w:rPr></w:pPr></w:pPrChange></w:pPr></w:p>"#
    );
    assert_eq!(
        revisions(&[&one, &snapshot]),
        expected(&[(C::RevisionIdDuplicate, "1")])
    );
    let only_snapshot = format!(
        r#"<w:p><w:pPr><w:pPrChange w:id="2" {REV}><w:pPr><w:rPr><w:ins w:id="1" {REV}/></w:rPr></w:pPr></w:pPrChange></w:pPr></w:p>"#
    );
    assert_eq!(revisions(&[&one, &only_snapshot]), expected(&[]));
    // the branch of an alternate content that Word does not take
    let fallback = format!(
        r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:ins w:id="1" {REV}><w:r/></w:ins></mc:Choice><mc:Fallback><w:ins w:id="1" {REV}><w:r/></w:ins></mc:Fallback></mc:AlternateContent></w:r></w:p>"#
    );
    assert_eq!(
        revisions(&[&fallback]),
        expected(&[]),
        "the fallback is not read"
    );
    // an id that is not a whole number is not an id
    let words = format!(
        r#"<w:p><w:ins w:id="x" {REV}><w:r/></w:ins><w:ins w:id="x" {REV}><w:r/></w:ins></w:p>"#
    );
    assert_eq!(revisions(&[&words]), expected(&[]));
    // and a revision and a bookmark may use one number
    let mark = r#"<w:p><w:bookmarkStart w:id="1" w:name="b"/><w:bookmarkEnd w:id="1"/></w:p>"#;
    assert_eq!(revisions(&[&one, mark]), expected(&[]));
    assert_eq!(part(mark), expected(&[]));
}

#[test]
fn a_bookmark_id_is_unique_within_a_part() {
    let mark = |id: u32, name: &str| {
        format!(r#"<w:bookmarkStart w:id="{id}" w:name="{name}"/><w:bookmarkEnd w:id="{id}"/>"#)
    };
    assert_eq!(
        part(&format!("<w:p>{}{}</w:p>", mark(1, "a"), mark(2, "b"))),
        expected(&[])
    );
    assert_eq!(
        part(&format!(
            "<w:p>{}</w:p><w:p>{}</w:p>",
            mark(1, "a"),
            mark(1, "b")
        )),
        expected(&[(C::BookmarkIdDuplicate, "word/document.xml#1")]),
        "two starts with one id, in two paragraphs"
    );
}

#[test]
fn a_bookmark_has_a_start_and_an_end() {
    let part_of = |body: &str| part(&format!("<w:p>{body}</w:p>"));
    assert_eq!(
        part_of(r#"<w:bookmarkStart w:id="3" w:name="a"/>"#),
        expected(&[(C::BookmarkUnpaired, "word/document.xml#3")])
    );
    assert_eq!(
        part_of(r#"<w:bookmarkEnd w:id="3"/>"#),
        expected(&[(C::BookmarkUnpaired, "word/document.xml#3")])
    );
    // a bookmark that spans paragraphs is paired, and so is one between blocks
    assert_eq!(
        part(r#"<w:p><w:bookmarkStart w:id="3" w:name="a"/></w:p><w:bookmarkEnd w:id="3"/><w:p/>"#),
        expected(&[])
    );
    // an end before its start is still a pair
    assert_eq!(
        part_of(r#"<w:bookmarkEnd w:id="3"/><w:bookmarkStart w:id="3" w:name="a"/>"#),
        expected(&[])
    );
    // a second start and one end do not pair
    assert_eq!(
        part_of(
            r#"<w:bookmarkStart w:id="3" w:name="a"/><w:bookmarkStart w:id="3" w:name="b"/><w:bookmarkEnd w:id="3"/>"#
        ),
        expected(&[
            (C::BookmarkIdDuplicate, "word/document.xml#3"),
            (C::BookmarkUnpaired, "word/document.xml#3"),
        ])
    );
}

#[test]
fn a_move_has_both_ranges_and_both_names() {
    let from = |id: u32, name: &str| {
        format!(
            r#"<w:moveFromRangeStart w:id="{id}" w:name="{name}" {REV}/><w:moveFromRangeEnd w:id="{id}"/>"#
        )
    };
    let to = |id: u32, name: &str| {
        format!(
            r#"<w:moveToRangeStart w:id="{id}" w:name="{name}" {REV}/><w:moveToRangeEnd w:id="{id}"/>"#
        )
    };
    assert_eq!(
        part(&format!(
            "<w:p>{}</w:p><w:p>{}</w:p>",
            from(1, "m"),
            to(2, "m")
        )),
        expected(&[])
    );
    assert_eq!(
        part(&format!("<w:p>{}</w:p>", from(1, "m"))),
        expected(&[(C::MoveNameUnpaired, "word/document.xml#m")]),
        "a source with no destination"
    );
    assert_eq!(
        part(&format!("<w:p>{}</w:p>", to(2, "m"))),
        expected(&[(C::MoveNameUnpaired, "word/document.xml#m")]),
        "a destination with no source"
    );
    assert_eq!(
        part(&format!(
            r#"<w:p><w:moveFromRangeStart w:id="1" w:name="m" {REV}/>{}</w:p>"#,
            to(2, "m")
        )),
        expected(&[(C::MoveRangeUnpaired, "word/document.xml#from 1")]),
        "a start with no end"
    );
    assert_eq!(
        part(&format!(
            r#"<w:p>{}<w:moveToRangeEnd w:id="9"/></w:p>"#,
            from(1, "m")
        )),
        expected(&[
            (C::MoveRangeUnpaired, "word/document.xml#to 9"),
            (C::MoveNameUnpaired, "word/document.xml#m"),
        ]),
        "an end with no start, and the destination that is missing"
    );
}

#[test]
fn the_census_does_not_read_the_untaken_branch_or_an_ignorable_element() {
    let alternate = r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:bookmarkStart w:id="1" w:name="c"/><w:bookmarkEnd w:id="1"/></mc:Choice><mc:Fallback><w:bookmarkStart w:id="1" w:name="f"/></mc:Fallback></mc:AlternateContent></w:r></w:p>"#;
    assert_eq!(part(alternate), expected(&[]));
    let ignorable =
        r#"<w:p mc:Ignorable="x"><x:note><w:bookmarkStart w:id="1" w:name="c"/></x:note></w:p>"#;
    assert_eq!(part(ignorable), expected(&[]));
}

#[test]
fn a_census_is_a_cache_over_the_generation_of_its_part() {
    use super::document::census_of;
    use crate::api::limits::Limits;
    use crate::state::State;
    use stetdoc_testkit::fixtures::{parts, zip::Zip};
    let doc = parts::Doc::new().body(&ins(1)).bytes();
    let state = State::open(
        Zip::new(parts::entries_doc(&doc)).build(),
        &Limits::default(),
    )
    .expect("opens");
    let view = state.parts();
    let id = view.id("word/document.xml").expect("the main part");
    let tree = view.tree(id).expect("parses");
    let first = census_of(&view, id, tree).expect("built");
    let second = census_of(&view, id, tree).expect("cached");
    assert!(
        Arc::ptr_eq(&first, &second),
        "the second read is the first value"
    );
    assert_eq!(*first, Census::of(tree));
}
