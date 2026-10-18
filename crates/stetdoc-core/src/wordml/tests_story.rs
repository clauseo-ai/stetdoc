//! Tests of the block scan, the fingerprints, story discovery and the id readers: known answers
//! from hand-written XML, one rule at a time.

use super::testing::{document, keys, parse, part, story};
use super::*;
use crate::api::error::ErrorCode;
use crate::xml::SpanTree;
use std::collections::HashSet;
use std::sync::Arc;

use BlockKind::{Paragraph as P, Row as R, Table as T};

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

fn part_name(name: &str) -> Arc<str> {
    Arc::from(name)
}

fn scan_body(body: &str) -> (SpanTree, Story) {
    let tree = document(body);
    let story = Story::scan(&part_name("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (tree, story)
}

fn kinds(s: &Story) -> Vec<BlockKind> {
    s.blocks().iter().map(|e| e.kind).collect()
}

fn parents(s: &Story) -> Vec<Option<u32>> {
    s.blocks().iter().map(|e| e.parent).collect()
}

fn flags(s: &Story) -> Vec<u8> {
    s.blocks().iter().map(|e| e.flags).collect()
}

// ---- order and structure --------------------------------------------------------------------

#[test]
fn blocks_come_in_pre_order_with_a_table_before_its_rows_and_a_row_before_its_blocks() {
    let s = story(
        "<w:p/>\
         <w:tbl><w:tblPr/><w:tblGrid/>\
           <w:tr><w:trPr/><w:tc><w:tcPr/><w:p/><w:p/></w:tc><w:tc><w:p/></w:tc></w:tr>\
           <w:tr><w:tc><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:tc></w:tr>\
         </w:tbl>\
         <w:p/><w:sectPr/>",
    );
    assert_eq!(kinds(&s), [P, T, R, P, P, P, R, T, R, P, P]);
    let some = |i: u32| Some(i);
    assert_eq!(
        parents(&s),
        [
            None,
            None,
            some(1),
            some(2),
            some(2),
            some(2),
            some(1),
            some(6),
            some(7),
            some(8),
            None
        ]
    );
    assert!(s.opaque().is_empty(), "the final sectPr is not content");
    assert!(s.anomalies().is_empty());
}

#[test]
fn a_cell_is_not_an_entry_and_an_empty_body_has_no_blocks() {
    assert_eq!(kinds(&story("<w:tbl><w:tr><w:tc/></w:tr></w:tbl>")), [T, R]);
    assert!(story("").blocks().is_empty());
}

#[test]
fn tables_nest_to_depth_four() {
    let inner = "<w:p/>";
    let mut xml = inner.to_string();
    for _ in 0..4 {
        xml = format!("<w:tbl><w:tr><w:tc>{xml}</w:tc></w:tr></w:tbl>");
    }
    let s = story(&xml);
    assert_eq!(s.blocks().len(), 4 * 2 + 1);
    let last = s.blocks().last().unwrap();
    assert_eq!(last.kind, P);
    assert_eq!(last.parent, Some(7));
}

#[test]
fn index_of_and_key_find_a_block_by_its_element() {
    let s = story("<w:p/><w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>");
    for (i, e) in s.blocks().iter().enumerate() {
        assert_eq!(s.index_of(e.node), Some(i));
        let key = s.key(e);
        assert_eq!(key.node(), e.node);
        assert_eq!(&**key.part(), "word/document.xml");
    }
    let all: HashSet<BlockKey> = keys(&s).into_iter().collect();
    assert_eq!(all.len(), s.blocks().len(), "every key is distinct");
}

// ---- transparent containers -----------------------------------------------------------------

#[test]
fn block_level_wrappers_are_walked_through_and_add_no_level() {
    let s = story(
        "<w:sdt><w:sdtPr/><w:sdtContent><w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:sdtContent></w:sdt>\
         <w:customXml w:uri=\"u\"><w:customXmlPr/><w:p/></w:customXml>\
         <w:smartTag><w:smartTagPr/><w:p/></w:smartTag>\
         <w:ins><w:p/></w:ins><w:moveFrom><w:p/></w:moveFrom>",
    );
    assert_eq!(kinds(&s), [P, T, R, P, P, P, P, P]);
    assert_eq!(
        parents(&s),
        [None, None, Some(1), Some(2), None, None, None, None]
    );
    // a block inside a content control says so, outside it does not
    assert_eq!(flags(&s), [IN_CONTROL, 0, 0, IN_CONTROL, 0, 0, 0, 0]);
}

#[test]
fn rows_and_cells_in_content_controls_are_walked_through() {
    let s = story(
        "<w:tbl><w:sdt><w:sdtContent><w:tr><w:sdt><w:sdtContent><w:tc><w:p/></w:tc></w:sdtContent></w:sdt></w:tr></w:sdtContent></w:sdt></w:tbl>",
    );
    assert_eq!(kinds(&s), [T, R, P]);
    assert_eq!(parents(&s), [None, Some(0), Some(1)]);
    assert_eq!(flags(&s), [0, 0, IN_CONTROL]);
}

#[test]
fn an_alternate_content_is_read_through_the_branch_word_takes() {
    let s = story(
        "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:p/></mc:Choice><mc:Fallback><w:p/><w:p/></mc:Fallback></mc:AlternateContent>\
         <mc:AlternateContent><mc:Choice Requires=\"q\" xmlns:q=\"urn:not-understood\"><w:p/></mc:Choice><mc:Fallback><w:p/><w:p/></mc:Fallback></mc:AlternateContent>\
         <mc:AlternateContent><mc:Choice Requires=\"unbound\"><w:p/></mc:Choice></mc:AlternateContent>",
    );
    assert_eq!(
        kinds(&s),
        [P, P, P],
        "one from the Choice, two from the Fallback, none"
    );
}

// ---- what is not a block --------------------------------------------------------------------

#[test]
fn markers_and_properties_are_skipped_and_unknown_content_is_listed_as_opaque() {
    let s = story(
        "<w:bookmarkStart w:id=\"1\" w:name=\"a\"/><w:p/><w:bookmarkEnd w:id=\"1\"/>\
         <w:permStart w:id=\"2\"/><w:commentRangeStart w:id=\"3\"/>\
         <w:altChunk r:id=\"rId9\"/><m:oMathPara/><x:foo xmlns:x=\"urn:x\"/><w:r/>\
         <w:p/><w:altChunk r:id=\"rId10\"/>",
    );
    assert_eq!(kinds(&s), [P, P]);
    let opaque: Vec<(OpaqueKind, Option<usize>)> =
        s.opaque().iter().map(|o| (o.kind, o.after)).collect();
    assert_eq!(
        opaque,
        [
            (OpaqueKind::AltChunk, Some(0)),
            (OpaqueKind::Math, Some(0)),
            (OpaqueKind::Unknown, Some(0)),
            (OpaqueKind::Unknown, Some(0)),
            (OpaqueKind::AltChunk, Some(1)),
        ]
    );
    assert!(
        s.anomalies().is_empty(),
        "unknown content at block level is opaque, not an anomaly"
    );
}

#[test]
fn opaque_content_before_any_block_has_no_predecessor() {
    let s = story("<w:altChunk r:id=\"r\"/><w:p/>");
    assert_eq!(s.opaque().len(), 1);
    assert_eq!(s.opaque()[0].after, None);
}

#[test]
fn a_misplaced_element_in_a_table_or_a_row_is_opaque_and_an_anomaly() {
    let s = story("<w:tbl><w:p/><w:tr><w:tc><w:p/></w:tc><w:p/></w:tr></w:tbl>");
    assert_eq!(kinds(&s), [T, R, P]);
    assert_eq!(s.opaque().len(), 2);
    assert_eq!(
        s.anomalies().iter().map(|a| a.kind).collect::<Vec<_>>(),
        [AnomalyKind::MisplacedElement, AnomalyKind::MisplacedElement]
    );
}

#[test]
fn a_table_inside_a_paragraph_is_not_a_block() {
    let s = story("<w:p><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:p>");
    assert_eq!(kinds(&s), [P]);
}

// ---- what a paragraph says about itself ------------------------------------------------------

#[test]
fn a_paragraph_reports_its_section_break_and_its_revised_mark() {
    let s = story(
        "<w:p><w:pPr><w:sectPr/></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:ins w:id=\"1\"/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:del w:id=\"2\"/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:moveFrom w:id=\"3\"/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:moveTo w:id=\"4\"/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:ins w:id=\"5\"/></w:rPr><w:sectPr/></w:pPr></w:p>\
         <w:p><w:r/><w:pPr><w:sectPr/></w:pPr></w:p>\
         <w:p/>",
    );
    assert_eq!(
        flags(&s),
        [
            SECTION_BREAK,
            MARK_INSERTED,
            MARK_DELETED,
            MARK_MOVED_FROM,
            MARK_MOVED_TO,
            MARK_INSERTED | SECTION_BREAK,
            0,
            0
        ]
    );
    assert!(s.blocks()[5].has(SECTION_BREAK) && s.blocks()[5].has(MARK_INSERTED));
}

// ---- stories --------------------------------------------------------------------------------

#[test]
fn only_the_body_can_fail_and_only_for_its_root_or_its_body() {
    let not_a_document = part("word/document.xml", "hdr", "<w:p/>");
    let e = Story::scan(
        &part_name("word/document.xml"),
        StoryKind::Body,
        &not_a_document,
    )
    .unwrap_err();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(
        e.message().starts_with("word/document.xml:"),
        "{}",
        e.message()
    );
    let no_body = part("word/document.xml", "document", "");
    let e = Story::scan(&part_name("word/document.xml"), StoryKind::Body, &no_body).unwrap_err();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
}

#[test]
fn another_story_with_the_wrong_root_is_empty_with_an_anomaly() {
    for kind in [
        StoryKind::Header,
        StoryKind::Footer,
        StoryKind::Footnotes,
        StoryKind::Endnotes,
        StoryKind::Comments,
    ] {
        let tree = part("word/x.xml", "document", "<w:p/>");
        let s = Story::scan(&part_name("word/x.xml"), kind, &tree).unwrap();
        assert!(s.blocks().is_empty(), "{kind:?}");
        assert_eq!(s.anomalies().len(), 1);
        assert_eq!(s.anomalies()[0].kind, AnomalyKind::WrongRoot);
    }
}

#[test]
fn headers_and_footers_are_stories_of_their_own_blocks() {
    let h = part(
        "word/header1.xml",
        "hdr",
        "<w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    let s = Story::scan(&part_name("word/header1.xml"), StoryKind::Header, &h).unwrap();
    assert_eq!(kinds(&s), [P, T, R, P]);
    assert_eq!(s.kind(), StoryKind::Header);
    let f = part("word/footer1.xml", "ftr", "<w:p/>");
    assert_eq!(
        Story::scan(&part_name("word/footer1.xml"), StoryKind::Footer, &f)
            .unwrap()
            .blocks()
            .len(),
        1
    );
}

#[test]
fn notes_are_units_and_separator_notes_are_skipped() {
    let tree = part(
        "word/footnotes.xml",
        "footnotes",
        "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p/></w:footnote>\
         <w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p/></w:footnote>\
         <w:footnote w:type=\"continuationNotice\" w:id=\"1\"><w:p/></w:footnote>\
         <w:footnote w:id=\"2\"><w:p/><w:p/></w:footnote>\
         <w:footnote w:id=\"3\"><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:footnote>\
         <w:footnote><w:p/></w:footnote>",
    );
    let s = Story::scan(
        &part_name("word/footnotes.xml"),
        StoryKind::Footnotes,
        &tree,
    )
    .unwrap();
    assert_eq!(
        s.units().iter().map(|u| u.id).collect::<Vec<_>>(),
        [Some(2), Some(3), None]
    );
    assert!(s.units().iter().all(|u| u.kind == UnitKind::Footnote));
    let units: Vec<u32> = s.blocks().iter().map(|e| e.unit).collect();
    assert_eq!(units, [0, 0, 1, 1, 1, 2]);
    assert_eq!(kinds(&s), [P, P, T, R, P, P]);
    assert_eq!(s.unit_of(&s.blocks()[2]).unwrap().id, Some(3));
    assert_eq!(
        s.anomalies().iter().map(|a| a.kind).collect::<Vec<_>>(),
        [AnomalyKind::UnitWithoutId]
    );
}

#[test]
fn comments_carry_their_author_initials_and_date() {
    let tree = part(
        "word/comments.xml",
        "comments",
        "<w:comment w:id=\"0\" w:author=\"Ada\" w:initials=\"A\" w:date=\"2025-01-02T03:04:05Z\"><w:p/></w:comment>\
         <w:comment w:id=\"1\" w:author=\"Bo\"><w:p/></w:comment>",
    );
    let s = Story::scan(&part_name("word/comments.xml"), StoryKind::Comments, &tree).unwrap();
    let u = s.units();
    assert_eq!(u.len(), 2);
    assert_eq!(
        (
            u[0].author.as_deref(),
            u[0].initials.as_deref(),
            u[0].date.as_deref()
        ),
        (Some("Ada"), Some("A"), Some("2025-01-02T03:04:05Z"))
    );
    assert_eq!(
        (
            u[1].author.as_deref(),
            u[1].initials.as_deref(),
            u[1].date.as_deref()
        ),
        (Some("Bo"), None, None)
    );
    assert_eq!(u[1].kind, UnitKind::Comment);
}

#[test]
fn the_strict_dialect_scans_the_same() {
    let strict = parse(
        "word/document.xml",
        "<w:document xmlns:w=\"http://purl.oclc.org/ooxml/wordprocessingml/main\"><w:body>\
         <w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:sdt><w:sdtContent><w:p/></w:sdtContent></w:sdt></w:body></w:document>",
    );
    let s = Story::scan(&part_name("word/document.xml"), StoryKind::Body, &strict).unwrap();
    assert_eq!(kinds(&s), [P, T, R, P, P]);
}

#[test]
fn nesting_far_beyond_a_stack_is_fine() {
    let depth = 100;
    let mut xml = String::from("<w:p/>");
    for _ in 0..depth {
        xml = format!("<w:sdt><w:sdtContent>{xml}</w:sdtContent></w:sdt>");
    }
    let s = story(&xml);
    assert_eq!(s.blocks().len(), 1);
    assert!(s.blocks()[0].has(IN_CONTROL));
}

// ---- refresh --------------------------------------------------------------------------------

#[test]
fn a_refresh_equals_a_scan_from_scratch() {
    let mut tree = document(
        "<w:p><w:r><w:t>a</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    let name = part_name("word/document.xml");
    let before = Story::scan(&name, StoryKind::Body, &tree).unwrap();
    let body = tree.root().first_child().unwrap().id();
    {
        let mut ed = tree.edit();
        let p = ed.create_element(W_NS, "p").unwrap();
        ed.insert_before(body, Some(before.blocks()[0].node), p)
            .unwrap();
    }
    let refreshed = Story::refresh(&before, &tree, &[]).unwrap().story;
    let scratch = Story::scan(&name, StoryKind::Body, &tree).unwrap();
    assert!(refreshed.same_as(&scratch));
    assert_eq!(refreshed.blocks().len(), before.blocks().len() + 1);
}

#[test]
fn a_block_level_alternate_content_with_no_usable_branch_is_counted() {
    // Proves: the scan reports `NoBranch` for an `mc:AlternateContent` between blocks that Word would
    // read no branch of, as the text model does inside a paragraph, and lists no entry for it.
    let s = story(
        "<w:p/><mc:AlternateContent><mc:Choice Requires=\"zz\" xmlns:zz=\"urn:no\"><w:p/></mc:Choice></mc:AlternateContent><w:p/>",
    );
    assert_eq!(s.blocks().len(), 2);
    let kinds: Vec<AnomalyKind> = s.anomalies().iter().map(|a| a.kind).collect();
    assert_eq!(kinds, [AnomalyKind::NoBranch]);
}
