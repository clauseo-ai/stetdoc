//! Tests of the revision reader: every kind of revision element, what a property change holds
//! inside it, the parts that are not stories, and the annotation ids those parts carry.

use super::testing::{document, part};
use super::tests_story_ids::{T_BASE, rel};
use super::*;
use std::sync::Arc;

fn revisions_of(body: &str) -> Vec<Revision> {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    story.revisions(&tree)
}

fn kinds(revisions: &[Revision]) -> Vec<RevKind> {
    revisions.iter().map(|r| r.kind).collect()
}

#[test]
fn every_kind_of_revision_element_is_classified_by_its_name_and_its_parent() {
    // Proves: each revision element reads as its own kind, in document order, with its id, author
    // and date. A `w:ins` is a different kind in a paragraph mark, a row and a numbering.
    let r = revisions_of(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/><w:ins w:id=\"10\" w:author=\"A\" w:date=\"2024-05-01T10:00:00Z\"/></w:numPr>\
         <w:rPr><w:ins w:id=\"11\" w:author=\"A\"/><w:del w:id=\"12\"/><w:moveFrom w:id=\"13\"/><w:moveTo w:id=\"14\"/></w:rPr>\
         <w:pPrChange w:id=\"15\" w:author=\"B\"><w:pPr/></w:pPrChange></w:pPr>\
         <w:ins w:id=\"16\"><w:r><w:t>a</w:t></w:r></w:ins><w:del w:id=\"17\"><w:r><w:delText>b</w:delText></w:r></w:del>\
         <w:moveFrom w:id=\"18\"/><w:moveTo w:id=\"19\"/><w:r><w:rPr><w:rPrChange w:id=\"20\"/></w:rPr></w:r></w:p>\
         <w:tbl><w:tblPr><w:tblPrChange w:id=\"21\"/></w:tblPr><w:tblPrEx><w:tblPrExChange w:id=\"22\"/></w:tblPrEx><w:tblGrid><w:tblGridChange w:id=\"23\"/></w:tblGrid>\
         <w:tr><w:trPr><w:ins w:id=\"24\"/><w:del w:id=\"25\"/><w:trPrChange w:id=\"26\"/></w:trPr>\
         <w:tc><w:tcPr><w:cellIns w:id=\"27\"/><w:cellDel w:id=\"28\"/><w:cellMerge w:id=\"29\"/><w:tcPrChange w:id=\"30\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>\
         <w:sectPr><w:sectPrChange w:id=\"31\"/></w:sectPr>",
    );
    use RevKind::*;
    assert_eq!(
        kinds(&r),
        [
            NumPrIns,
            MarkIns,
            MarkDel,
            MarkMoveFrom,
            MarkMoveTo,
            ParagraphProps,
            Ins,
            Del,
            MoveFrom,
            MoveTo,
            RunProps,
            TableProps,
            TableExProps,
            TableGrid,
            RowIns,
            RowDel,
            RowProps,
            CellIns,
            CellDel,
            CellMerge,
            CellProps,
            SectionProps
        ]
    );
    let ids: Vec<Option<i64>> = r.iter().map(|x| x.id).collect();
    let want: Vec<Option<i64>> = [
        10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
    ]
    .map(Some)
    .to_vec();
    assert_eq!(ids, want);
    assert_eq!(r[0].author.as_deref(), Some("A"));
    assert_eq!(r[0].date.as_deref(), Some("2024-05-01T10:00:00Z"));
    assert_eq!((r[2].author.clone(), r[2].date.clone()), (None, None));
    assert!(r.iter().all(|x| x.branch == Branch::Outside));
}

#[test]
fn what_a_property_change_holds_is_a_copy_and_not_a_revision() {
    // Proves: a revision element inside the old properties of a property change, which Word 2010 and
    // later write for the paragraph mark, is not listed, though the change itself is.
    let r = revisions_of(
        "<w:p><w:pPr><w:numPr><w:numId w:val=\"1\"/><w:ins w:id=\"2\"/></w:numPr><w:rPr><w:ins w:id=\"1\" w:author=\"A\"/></w:rPr>\
         <w:pPrChange w:id=\"3\" w:author=\"B\"><w:pPr><w:numPr><w:ins w:id=\"2\"/></w:numPr><w:rPr><w:ins w:id=\"1\" w:author=\"A\"/><w:del w:id=\"9\"/></w:rPr></w:pPr></w:pPrChange></w:pPr>\
         <w:r><w:rPr><w:b/><w:rPrChange w:id=\"4\"><w:rPr><w:ins w:id=\"8\"/><w:rPrChange w:id=\"7\"/></w:rPr></w:rPrChange></w:rPr></w:r></w:p>\
         <w:tbl><w:tr><w:trPr><w:trPrChange w:id=\"5\"><w:trPr><w:ins w:id=\"6\"/></w:trPr></w:trPrChange></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    use RevKind::*;
    assert_eq!(
        kinds(&r),
        [NumPrIns, MarkIns, ParagraphProps, RunProps, RowProps]
    );
    let ids: Vec<Option<i64>> = r.iter().map(|x| x.id).collect();
    assert_eq!(ids, [Some(2), Some(1), Some(3), Some(4), Some(5)]);
}

#[test]
fn custom_xml_range_markers_are_revisions_with_an_id_and_the_start_has_an_author() {
    // Proves: the markers around a custom XML element, at the block level and in a paragraph, are
    // read, because they are the only record that the element was inserted, deleted or moved.
    let r = revisions_of(
        "<w:customXmlInsRangeStart w:id=\"1\" w:author=\"A\" w:date=\"2024-01-01T00:00:00Z\"/><w:p>\
         <w:customXmlDelRangeStart w:id=\"2\" w:author=\"B\"/><w:customXmlDelRangeEnd w:id=\"2\"/>\
         <w:customXmlMoveFromRangeStart w:id=\"3\" w:author=\"C\"/><w:customXmlMoveFromRangeEnd w:id=\"3\"/>\
         <w:customXmlMoveToRangeStart w:id=\"4\" w:author=\"D\"/><w:customXmlMoveToRangeEnd w:id=\"4\"/></w:p>\
         <w:customXmlInsRangeEnd w:id=\"1\"/>",
    );
    use RevKind::*;
    assert_eq!(
        kinds(&r),
        [
            CustomXmlInsStart,
            CustomXmlDelStart,
            CustomXmlDelEnd,
            CustomXmlMoveFromStart,
            CustomXmlMoveFromEnd,
            CustomXmlMoveToStart,
            CustomXmlMoveToEnd,
            CustomXmlInsEnd
        ]
    );
    assert_eq!(r[0].author.as_deref(), Some("A"));
    assert_eq!(r[0].date.as_deref(), Some("2024-01-01T00:00:00Z"));
    assert_eq!((r[7].id, r[7].author.clone()), (Some(1), None));
}

#[test]
fn a_table_exception_property_change_is_its_own_kind() {
    let r = revisions_of(
        "<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tblPrEx><w:tblPrExChange w:id=\"4\" w:author=\"A\"><w:tblPrEx/></w:tblPrExChange></w:tblPrEx><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    assert_eq!(kinds(&r), [RevKind::TableExProps]);
}

#[test]
fn a_part_that_is_not_a_story_has_its_revisions_read_too() {
    // Proves: the property changes in styles and numbering are listed by `part_revisions`, and what the
    // old properties hold is not.
    let styles = part(
        "word/styles.xml",
        "styles",
        "<w:style w:type=\"paragraph\" w:styleId=\"X\"><w:pPr><w:rPr><w:ins w:id=\"7\"/></w:rPr>\
         <w:pPrChange w:id=\"8\" w:author=\"A\"><w:pPr><w:rPr><w:ins w:id=\"7\"/></w:rPr></w:pPr></w:pPrChange></w:pPr>\
         <w:rPr><w:rPrChange w:id=\"9\"/></w:rPr></w:style>",
    );
    let r = part_revisions(&styles);
    assert_eq!(
        kinds(&r),
        [RevKind::MarkIns, RevKind::ParagraphProps, RevKind::RunProps]
    );
    let numbering = part(
        "word/numbering.xml",
        "numbering",
        "<w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:pPr><w:pPrChange w:id=\"20\"><w:pPr/></w:pPrChange></w:pPr></w:lvl></w:abstractNum>",
    );
    assert_eq!(
        kinds(&part_revisions(&numbering)),
        [RevKind::ParagraphProps]
    );
    assert!(part_revisions(&part("word/styles.xml", "styles", "")).is_empty());
}

#[test]
fn a_revision_in_an_alternate_content_knows_its_branch() {
    let r = revisions_of(
        "<w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:ins w:id=\"1\"/></mc:Choice>\
         <mc:Fallback><w:ins w:id=\"2\"/></mc:Fallback></mc:AlternateContent></w:r><w:ins w:id=\"3\"/></w:p>",
    );
    let branches: Vec<Branch> = r.iter().map(|x| x.branch).collect();
    assert_eq!(branches, [Branch::Taken, Branch::Untaken, Branch::Outside]);
}

// ---- the parts whose ids a new annotation must avoid ---------------------------------------------

#[test]
fn annotation_ids_are_read_from_the_stories_and_from_styles_and_numbering() {
    // Proves: `id_parts(AnnotationId)` lists the stories, then the styles part, then the numbering
    // part, found by relationship type and each once, and the other kinds of id are unchanged.
    let stories = vec![
        StorySpec {
            part: Arc::from("word/document.xml"),
            kind: StoryKind::Body,
        },
        StorySpec {
            part: Arc::from("word/footnotes.xml"),
            kind: StoryKind::Footnotes,
        },
    ];
    let rels = Rels::new(vec![
        rel(
            "rId1",
            &format!("{T_BASE}numbering"),
            RelTarget::Part("word/numbering.xml".into()),
        ),
        rel(
            "rId2",
            &format!("{T_BASE}styles"),
            RelTarget::Part("word/styles.xml".into()),
        ),
        rel(
            "rId3",
            &format!("{T_BASE}styles"),
            RelTarget::Part("word/styles2.xml".into()),
        ),
    ]);
    let names = |kind| -> Vec<String> {
        id_parts(kind, &stories, &rels)
            .iter()
            .map(|p| p.to_string())
            .collect()
    };
    assert_eq!(
        names(IdKind::AnnotationId),
        [
            "word/document.xml",
            "word/footnotes.xml",
            "word/styles.xml",
            "word/numbering.xml"
        ]
    );
    assert_eq!(
        names(IdKind::TextId),
        ["word/document.xml", "word/footnotes.xml"]
    );
    assert_eq!(
        names(IdKind::ParaId),
        ["word/document.xml", "word/footnotes.xml"]
    );
    let none = id_parts(IdKind::AnnotationId, &stories, &Rels::default());
    assert_eq!(none.len(), 2, "no relationship, no part");
    // the ids those parts carry are read like any other
    let styles = part(
        "word/styles.xml",
        "styles",
        "<w:style w:styleId=\"X\"><w:pPr><w:pPrChange w:id=\"41\"/></w:pPr><w:rPr><w:rPrChange w:id=\"42\"/></w:rPr></w:style>",
    );
    let mut ids = Vec::new();
    existing_ids(IdKind::AnnotationId, &styles, &mut ids);
    assert_eq!(ids, [41, 42]);
}

// ---- base properties and the largest list number ------------------------------------------------

#[test]
fn base_properties_leave_out_the_mark_the_section_break_and_the_old_properties() {
    // Proves: two paragraphs formatted alike have equal base properties whatever their mark, section
    // break and tracked history say, and a different style makes them unequal.
    let tree = document(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:jc w:val=\"center\"/><w:rPr><w:b/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:jc w:val=\"center\"/><w:rPr><w:i/></w:rPr><w:sectPr/>\
         <w:pPrChange w:id=\"1\"><w:pPr><w:pStyle w:val=\"Normal\"/></w:pPr></w:pPrChange></w:pPr></w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"Normal\"/><w:jc w:val=\"center\"/></w:pPr></w:p><w:p/>",
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let base = |i| story.paragraph(&tree, i).unwrap().base_props();
    assert_eq!(base(0), base(1));
    assert_ne!(base(0), base(2));
    let first = base(0);
    let names: Vec<&str> = first.iter().map(|p| &*p.name).collect();
    assert_eq!(names, ["pStyle", "jc"]);
    assert!(base(3).is_empty());
}

#[test]
fn the_largest_num_id_is_zero_with_no_list_and_the_highest_otherwise() {
    let read = |body: &str| NumberingDefs::read(&part("word/numbering.xml", "numbering", body));
    assert_eq!(read("").max_num_id(), 0);
    let num = |id: i64| format!("<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"1\"/></w:num>");
    let defs = read(&format!("{}{}{}", num(3), num(12), num(5)));
    assert_eq!(defs.max_num_id(), 12);
    // a list number is never below zero, so a new one starts at 1 even if the file has a negative id
    assert_eq!(read(&num(-5)).max_num_id(), 0);
}

#[test]
fn a_w_t_in_a_move_source_is_no_anomaly_and_one_in_a_deletion_inside_it_is() {
    // Word writes `w:t` in a `w:moveFrom`; a `w:t` in a `w:del` stays an anomaly wherever the `w:del` is.
    let m = super::tests_text::para(
        "<w:moveFrom w:id=\"1\"><w:r><w:t>a</w:t></w:r></w:moveFrom><w:r><w:t>b</w:t></w:r>",
    );
    assert_eq!(super::tests_text::texts(&m), ["b", "ab", "ab"]);
    assert!(m.anomalies().is_empty(), "{:?}", m.anomalies());
    let m = super::tests_text::para(
        "<w:moveFrom w:id=\"1\"><w:del w:id=\"2\"><w:r><w:t>a</w:t></w:r></w:del></w:moveFrom>",
    );
    let kinds: Vec<super::AnomalyKind> = m.anomalies().iter().map(|a| a.kind).collect();
    assert_eq!(kinds, [super::AnomalyKind::TextInDeletion]);
}
