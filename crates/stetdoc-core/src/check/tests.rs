//! Tests of `document` at its interface: a state is built from a small package, and the report is
//! what a caller of `Document::check` would read.

use super::*;
use crate::api::check::{CheckReport, Class, FindingCode};
use crate::api::limits::Limits;
use crate::state::State;
use crate::xml::{Bytes, NodeId, SpanTree};
use stetdoc_testkit::fixtures::{
    parts,
    zip::{Entry, Zip},
};

pub(super) const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:x="urn:example:unknown""#;

/// A part as a span tree.
pub(super) fn tree(xml: &str) -> SpanTree {
    SpanTree::parse(
        "word/document.xml",
        Bytes::from(xml.as_bytes()),
        &Limits::default(),
    )
    .expect("test xml parses")
}

/// A main part with this body.
pub(super) fn document_xml(body: &str) -> String {
    format!(r#"<w:document xmlns:w="{W}" {NS}><w:body>{body}</w:body></w:document>"#)
}

/// A state whose main part has this body, and no other part.
pub(super) fn state(body: &str) -> State {
    let doc = parts::Doc::new().body(body).bytes();
    State::open(
        Zip::new(parts::entries_doc(&doc)).build(),
        &Limits::default(),
    )
    .expect("opens")
}

/// A state with a main part, and the parts given as (name, relationship type tail, content type
/// tail, bytes), each related from the main part.
pub(super) fn state_with(body: &str, extra: &[(&str, &str, &str, &str)]) -> State {
    let doc = parts::Doc::new().body(body).bytes();
    let mut rels = String::new();
    let mut types = parts::content_types();
    for (i, (name, rel, content, _)) in extra.iter().enumerate() {
        rels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="{R_NS}/{rel}" Target="{}"/>"#,
            i + 20,
            name.trim_start_matches("word/")
        ));
        types = types.replace(
            "</Types>",
            &format!(
                r#"<Override PartName="/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{content}+xml"/></Types>"#
            ),
        );
    }
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    for (name, _, _, xml) in extra {
        entries.push(Entry::new(name, *xml));
    }
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    State::open(Zip::new(entries).build(), &Limits::default()).expect("opens")
}

/// The elements with this local name, in document order.
pub(super) fn named(tree: &SpanTree, local: &str) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root().id()];
    while let Some(id) = stack.pop() {
        let node = tree.node_ref(id);
        if node.local_name() == local {
            out.push(id);
        }
        let mark = stack.len();
        stack.extend(node.child_elements().map(|c| c.id()));
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    out
}

fn count(report: &CheckReport, code: FindingCode) -> usize {
    report
        .by_code
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(0, |(_, n)| *n)
}

#[test]
fn a_clean_document_has_no_findings() {
    let s = state(parts::BODY);
    let report = document(&s.parts());
    assert!(report.ok, "{report:?}");
    assert_eq!((report.violations, report.notes), (0, 0));
    assert!(report.findings.is_empty());
    assert_eq!((report.parts, report.unchecked), (1, 0));
}

#[test]
fn a_document_reports_what_the_schema_and_the_word_rules_find() {
    // a run before the paragraph's properties, text in a deletion, an edge space, an empty
    // property container, and a table with no row
    let body = r#"<w:p><w:r><w:t>a</w:t></w:r><w:pPr/></w:p>
        <w:p><w:del w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t> b</w:t></w:r></w:del></w:p>
        <w:tbl><w:tblPr/><w:tblGrid/></w:tbl>"#;
    let s = state(body);
    let report = document(&s.parts());
    assert!(!report.ok);
    for (code, n) in [
        (FindingCode::SchemaOutOfOrder, 1),
        (FindingCode::TextInDeletion, 1),
        (FindingCode::EdgeSpaceWithoutPreserve, 1),
        (FindingCode::EmptyProperties, 1),
        (FindingCode::TableWithoutRow, 1),
    ] {
        assert_eq!(count(&report, code), n, "{code:?} in {report:?}");
    }
    assert_eq!(
        report.violations,
        report.by_code.iter().map(|(_, n)| n).sum::<usize>()
    );
    // a finding in a paragraph names the paragraph
    let schema = report
        .findings
        .iter()
        .find(|f| f.code == FindingCode::SchemaOutOfOrder)
        .expect("listed");
    assert_eq!(schema.part, "word/document.xml");
    assert!(
        schema.block.as_deref().is_some_and(|b| b.starts_with('p')),
        "{schema:?}"
    );
    assert_eq!(schema.class, Class::Violation);
}

#[test]
fn the_report_is_the_same_every_time() {
    let body = r#"<w:p><w:r><w:t> a</w:t></w:r></w:p><w:p><w:pPr/><w:r><w:rPr/></w:r></w:p>"#;
    let s = state(body);
    assert_eq!(document(&s.parts()), document(&s.parts()));
}

#[test]
fn counts_are_complete_and_the_list_is_capped() {
    let body = "<w:p><w:pPr/></w:p>".repeat(CheckReport::MAX_FINDINGS + 100);
    let s = state(&body);
    let report = document(&s.parts());
    assert_eq!(
        count(&report, FindingCode::EmptyProperties),
        CheckReport::MAX_FINDINGS + 100
    );
    assert_eq!(report.violations, CheckReport::MAX_FINDINGS + 100);
    assert_eq!(report.findings.len(), CheckReport::MAX_FINDINGS);
}

#[test]
fn a_part_that_cannot_be_read_is_a_note_and_not_a_failure() {
    let s = state_with(
        parts::BODY,
        &[("word/styles.xml", "styles", "styles", "<w:styles></w:oops>")],
    );
    let report = document(&s.parts());
    assert_eq!(report.unchecked, 1);
    assert_eq!(report.parts, 1);
    assert!(report.ok, "a note is not a violation: {report:?}");
    assert_eq!(report.notes, 1);
    let note = &report.findings[0];
    assert_eq!(note.code, FindingCode::PartUnreadable);
    assert_eq!(note.part, "word/styles.xml");
    assert_eq!(note.class, Class::Note);
}

#[test]
fn the_parts_a_document_names_are_all_examined() {
    let notes = format!(r#"<w:footnotes xmlns:w="{W}"><w:footnote w:id="1"/></w:footnotes>"#);
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:style w:type="paragraph" w:styleId="A"><w:rPr/></w:style></w:styles>"#
    );
    let s = state_with(
        parts::BODY,
        &[
            ("word/footnotes.xml", "footnotes", "footnotes", &notes),
            ("word/styles.xml", "styles", "styles", &styles),
        ],
    );
    let report = document(&s.parts());
    assert_eq!(report.parts, 3);
    // the footnote with no block is a Word rule, read in a story, and the styles part is only
    // read by the schema: its empty property container is not a finding
    assert_eq!(
        count(&report, FindingCode::ContainerWithoutBlock),
        1,
        "{report:?}"
    );
    assert_eq!(
        count(&report, FindingCode::EmptyProperties),
        0,
        "{report:?}"
    );
}

#[test]
fn a_revision_id_used_in_two_story_parts_is_one_finding_of_the_document() {
    let header = format!(
        r#"<w:hdr xmlns:w="{W}"><w:p><w:ins w:id="7" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>h</w:t></w:r></w:ins></w:p></w:hdr>"#
    );
    let body = r#"<w:p><w:ins w:id="7" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>b</w:t></w:r></w:ins></w:p>"#;
    let s = state_with(body, &[("word/header1.xml", "header", "header", &header)]);
    let report = document(&s.parts());
    assert_eq!(
        count(&report, FindingCode::RevisionIdDuplicate),
        1,
        "{report:?}"
    );
    // and a bookmark id repeated across parts is not
    let header = format!(
        r#"<w:hdr xmlns:w="{W}"><w:p><w:bookmarkStart w:id="3" w:name="a"/><w:bookmarkEnd w:id="3"/></w:p></w:hdr>"#
    );
    let body = r#"<w:p><w:bookmarkStart w:id="3" w:name="b"/><w:bookmarkEnd w:id="3"/></w:p>"#;
    let s = state_with(body, &[("word/header1.xml", "header", "header", &header)]);
    let report = document(&s.parts());
    assert!(report.ok, "{report:?}");
}

#[test]
fn a_schema_finding_names_the_elements_it_is_about() {
    let body = r#"<w:p><w:r><w:t w:space="preserve">a</w:t><w:rPr><w:b w:val="maybe"/></w:rPr></w:r><w:pPr/></w:p>"#;
    let s = state(body);
    let report = document(&s.parts());
    let said = |code: FindingCode| {
        report
            .findings
            .iter()
            .find(|f| f.code == code)
            .map(|f| f.message.clone())
            .unwrap_or_else(|| panic!("no {code:?} in {report:?}"))
    };
    assert_eq!(
        said(FindingCode::SchemaUnknownAttribute),
        "w:space is not an attribute of w:t"
    );
    assert_eq!(
        said(FindingCode::SchemaInvalidAttributeValue),
        "w:val of w:b is \"maybe\", which is not a valid value"
    );
    let order: Vec<&str> = report
        .findings
        .iter()
        .filter(|f| f.code == FindingCode::SchemaOutOfOrder)
        .map(|f| f.message.as_str())
        .collect();
    assert!(order.contains(&"w:pPr is out of order in w:p"), "{order:?}");
    assert!(order.contains(&"w:rPr is out of order in w:r"), "{order:?}");
}
