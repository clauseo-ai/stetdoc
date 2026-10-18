//! The package rules P1 to P3, and the comment audit's codes. A package that `add_part` leaves
//! wrong cannot be made through the interface, so the rules are read on packages that were built
//! wrong, and the edit that adds a part is shown to carry what the base already had.

use super::Declared;
use super::document::listing;
use super::package::findings;
use super::tests::W;
use super::tests_edit::{accepted, say};
use super::tests_units::comments_part;
use crate::api::check::FindingCode as C;
use crate::api::limits::Limits;
use crate::state::State;
use stetdoc_testkit::fixtures::{
    parts,
    zip::{Entry, Zip},
};

const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HEADER_TYPE: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";

/// A package whose main part has a header, with the content types and the relationships given.
fn package(content_types: &str, rels: &str) -> State {
    let doc = parts::Doc::new().body(parts::BODY).bytes();
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), content_types);
    entries.push(Entry::new(
        "word/header1.xml",
        format!(r#"<w:hdr xmlns:w="{W}"><w:p/></w:hdr>"#),
    ));
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    State::open(Zip::new(entries).build(), &Limits::default()).expect("opens")
}

fn rels(items: &str) -> String {
    format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{items}</Relationships>"#
    )
}

fn header_rel(id: &str) -> String {
    format!(r#"<Relationship Id="{id}" Type="{R_NS}/header" Target="header1.xml"/>"#)
}

fn types_with_header() -> String {
    parts::content_types().replace(
        "</Types>",
        &format!(r#"<Override PartName="/word/header1.xml" ContentType="{HEADER_TYPE}"/></Types>"#),
    )
}

/// What the package rules say about the document, as code and subject.
fn said(s: &State) -> Vec<(C, String)> {
    let view = s.parts();
    let list = listing(&view).expect("lists").parts;
    findings(&view, &list)
        .expect("reads")
        .into_iter()
        .map(|(_, f)| match f.key {
            super::Key::Rule { code, subject } => (code, subject),
            super::Key::Schema(k) => panic!("a schema key: {k:?}"),
        })
        .collect()
}

#[test]
fn a_package_that_is_whole_has_no_finding() {
    let s = package(&types_with_header(), &rels(&header_rel("rId9")));
    assert_eq!(said(&s), []);
}

#[test]
fn a_part_with_neither_an_override_nor_a_default_has_no_content_type() {
    // no default for xml, and an override for the main part only
    let types = parts::content_types().replace(
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        "",
    );
    let s = package(&types, &rels(&header_rel("rId9")));
    assert_eq!(
        said(&s),
        [(C::PartWithoutContentType, "word/header1.xml".to_owned())]
    );
    // an override matches the name whatever its case, and a default matches the extension's
    let types = types.replace(
        "</Types>",
        &format!(r#"<Override PartName="/WORD/Header1.XML" ContentType="{HEADER_TYPE}"/></Types>"#),
    );
    assert_eq!(said(&package(&types, &rels(&header_rel("rId9")))), []);
}

#[test]
fn a_relationship_that_names_no_part_is_found_and_an_external_one_is_not() {
    let items = format!(
        r#"{}<Relationship Id="rId10" Type="{R_NS}/header" Target="missing.xml"/><Relationship Id="rId11" Type="{R_NS}/hyperlink" Target="https://example.com/" TargetMode="External"/>"#,
        header_rel("rId9")
    );
    let s = package(&types_with_header(), &rels(&items));
    assert_eq!(
        said(&s),
        [(
            C::RelationshipTargetMissing,
            "word/document.xml#rId10".to_owned()
        )]
    );
}

#[test]
fn two_relationships_of_one_part_may_not_share_an_id() {
    let items = format!("{}{}", header_rel("rId9"), header_rel("rId9"));
    let s = package(&types_with_header(), &rels(&items));
    assert_eq!(
        said(&s),
        [(
            C::RelationshipIdDuplicate,
            "word/document.xml#rId9".to_owned()
        )]
    );
}

#[test]
fn an_edit_that_adds_a_part_is_not_refused_for_the_package_problems_the_base_had() {
    let items = format!(
        r#"{}{}<Relationship Id="rId10" Type="{R_NS}/header" Target="missing.xml"/>"#,
        header_rel("rId9"),
        header_rel("rId9")
    );
    let types = parts::content_types().replace(
        r#"<Default Extension="xml" ContentType="application/xml"/>"#,
        "",
    );
    let s = package(&types, &rels(&items));
    assert_eq!(said(&s).len(), 3, "{:?}", said(&s));
    let mut tx = s.begin();
    tx.add_part(comments_part(
        r#"<w:comment w:id="0" w:author="a"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment>"#,
    ))
    .expect("adds");
    let said = say(&tx, &Declared::default());
    accepted(&said);
}
