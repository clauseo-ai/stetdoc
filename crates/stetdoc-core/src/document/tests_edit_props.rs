//! Step 7 through `Document::edit`: `set`, `format`, `paragraph`, `style` and `list`, their units, and the section target under
//! the scope guard.

use super::tests_edit::*;
use super::*;
use crate::api::check::FindingCode;
use crate::api::edit::Edit;
use crate::api::revision::ChangeKind;
use crate::threads::tests_support::Pkg;

/// The main part as it is now.
fn main_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

fn kinds(doc: &Document) -> Vec<ChangeKind> {
    doc.revisions(&RevisionFilter::default())
        .unwrap()
        .iter()
        .map(|r| r.kind)
        .collect()
}

fn reject_all(doc: &mut Document) {
    run(doc, r#"{"op":"reject","all":true}"#);
    assert!(kinds(doc).is_empty());
}

#[test]
fn format_writes_run_properties_as_a_tracked_change_and_a_reject_takes_it_back() {
    let mut doc = rich();
    let before = main_xml(&doc);
    let receipt = run(
        &mut doc,
        r##"{"op":"format","ref":"p4","find":"thirty days","bold":true,"color":"#1f4e79","size":"14pt","font":"Arial"}"##,
    );
    assert!(receipt.operations[0].changed);
    assert_eq!(kinds(&doc), [ChangeKind::RunFormat]);
    let xml = main_xml(&doc);
    for want in [
        "<w:b/>",
        r#"<w:color w:val="1F4E79"/>"#,
        r#"<w:sz w:val="28"/>"#,
        "w:rPrChange",
    ] {
        assert!(xml.contains(want), "{want} in {xml}");
    }
    assert!(doc.check().ok, "{:?}", doc.check());
    reject_all(&mut doc);
    assert_eq!(final_text(&doc, "p4"), "Payment is due within thirty days.");
    assert!(!main_xml(&doc).contains("<w:b/>"), "{}", main_xml(&doc));
    let _ = before;
}

#[test]
fn paragraph_merges_indents_and_spacing_in_human_units() {
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"paragraph","ref":"p4","align":"justify","indent_left":"0.5in","first_line":"0.25in","space_after":"6pt","line_spacing":"1.5","keep_next":true}"#,
    );
    let xml = main_xml(&doc);
    for want in [
        r#"<w:jc w:val="both"/>"#,
        r#"w:left="720""#,
        r#"w:firstLine="360""#,
        r#"w:after="120""#,
        r#"w:line="360""#,
        r#"w:lineRule="auto""#,
        "<w:keepNext/>",
        "w:pPrChange",
    ] {
        assert!(xml.contains(want), "{want} in {xml}");
    }
    // A second edit keeps the left indent and swaps the first line for a hanging one.
    run(
        &mut doc,
        r#"{"op":"paragraph","ref":"p4","hanging":"0.25in"}"#,
    );
    let xml = main_xml(&doc);
    assert!(
        xml.contains(r#"w:hanging="360""#) && xml.contains(r#"w:left="720""#),
        "{xml}"
    );
    assert!(doc.check().ok, "{:?}", doc.check());
    let e = fails(
        &mut doc,
        r#"{"op":"paragraph","ref":"p4","indent_left":"wide"}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
}

#[test]
fn set_names_any_container_and_the_last_section_is_refused_under_a_scope() {
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"set","ref":"p5","props":[{"name":"w:spacing","attrs":{"before":"12pt"}}]},
           {"op":"set","ref":"t1","props":[{"name":"jc","val":"center"}]},
           {"op":"set","ref":"r1","cell":1,"props":[{"name":"shd","attrs":{"val":"clear","fill":"FFFF00"}}]},
           {"op":"set","target":"section","section":1,"props":[{"name":"pgSz","attrs":{"w":"8.5in","h":"11in"}}]}"#,
    );
    let xml = main_xml(&doc);
    for want in [
        r#"w:before="240""#,
        r#"w:fill="FFFF00""#,
        r#"<w:pgSz w:h="15840" w:w="12240"/>"#,
        "w:sectPrChange",
        "w:tblPrChange",
        "w:tcPrChange",
    ] {
        assert!(xml.contains(want), "{want} in {xml}");
    }
    assert!(doc.check().ok, "{:?}", doc.check());
    reject_all(&mut doc);
    // The last section's properties name no paragraph.
    let mut doc = rich();
    let before = (doc.base(), doc.save().unwrap().bytes);
    let b = batch(
        &doc,
        r#"{"op":"set","target":"section","section":1,"props":[{"name":"pgSz","attrs":{"w":"8.5in"}}]}"#,
    );
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p10".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(e.message().contains("name no paragraph"), "{e}");
    assert_eq!(e.op_index(), Some(0));
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    // A paragraph out of scope is refused as any block is, and the names and targets are checked.
    let b = batch(
        &doc,
        r#"{"op":"set","ref":"p5","props":[{"name":"jc","val":"right"}]}"#,
    );
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    for (ops, code) in [
        (
            r#"{"op":"set","ref":"p4","target":"row","props":[{"name":"jc","val":"left"}]}"#,
            ErrorCode::InvalidPayload,
        ),
        (
            r#"{"op":"set","target":"section","section":9,"props":[{"name":"titlePg"}]}"#,
            ErrorCode::NotFound,
        ),
    ] {
        let e = fails(&mut doc, ops);
        assert_eq!(e.code(), code, "{ops}: {e}");
    }
}

const STYLES: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/></w:style><w:style w:type="character" w:styleId="Strong"><w:name w:val="Strong"/></w:style>"#;

const NUMBERING: &str = r#"<w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="(%2)"/></w:lvl></w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#;

fn listed() -> Document {
    let item = |t: &str| {
        format!(
            r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#
        )
    };
    let body = format!(
        "{}{}<w:p><w:r><w:t>Plain.</w:t></w:r></w:p>",
        item("One."),
        item("Two.")
    );
    let bytes = Pkg::new(&body).styles(STYLES).numbering(NUMBERING).bytes();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

#[test]
fn style_finds_a_style_by_id_or_by_name_and_format_a_character_style() {
    let mut doc = listed();
    run(&mut doc, r#"{"op":"style","ref":"p3","style":"Heading 1"}"#);
    assert!(
        main_xml(&doc).contains(r#"<w:pStyle w:val="Heading1"/>"#),
        "{}",
        main_xml(&doc)
    );
    run(
        &mut doc,
        r#"{"op":"format","ref":"p3","char_style":"strong"}"#,
    );
    assert!(main_xml(&doc).contains(r#"<w:rStyle w:val="Strong"/>"#));
    assert!(doc.check().ok, "{:?}", doc.check());
    // A style that does not exist is `invalid_payload`, and the message lists the nearest names (design 2.5).
    let e = fails(&mut doc, r#"{"op":"style","ref":"p3","style":"Title"}"#);
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(e.message().contains("The nearest are"), "{e}");
    assert!(
        e.message().contains("`Heading 1`") || e.message().contains("`heading 1`"),
        "{e}"
    );
    let e = fails(&mut doc, r#"{"op":"style","ref":"p3","style":"Strong"}"#);
    assert_eq!(
        e.code(),
        ErrorCode::InvalidPayload,
        "a character style is not a paragraph style: {e}"
    );
}

#[test]
fn list_changes_a_level_restarts_with_a_new_instance_and_removes() {
    let mut doc = listed();
    run(&mut doc, r#"{"op":"list","ref":"p2","level":1}"#);
    assert!(main_xml(&doc).contains(r#"<w:ilvl w:val="1"/>"#));
    run(&mut doc, r#"{"op":"list","ref":"p2","restart":5}"#);
    let xml = main_xml(&doc);
    assert!(xml.contains(r#"<w:numId w:val="2"/>"#), "{xml}");
    run(&mut doc, r#"{"op":"list","ref":"p1","remove":true}"#);
    assert!(main_xml(&doc).contains(r#"<w:numId w:val="0"/>"#));
    assert!(doc.check().ok, "{:?}", doc.check());
    let e = fails(&mut doc, r#"{"op":"list","ref":"p3","level":1}"#);
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    // A restart under a scope that holds the paragraph declares numbering.xml; one outside is refused.
    let mut doc = listed();
    let b = batch(&doc, r#"{"op":"list","ref":"p2","restart":3}"#);
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p2".to_string()]))
        .unwrap();
    let mut doc = listed();
    let b = batch(&doc, r#"{"op":"list","ref":"p2","restart":3}"#);
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p1".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
}

#[test]
fn no_property_operation_writes_or_removes_revision_markup() {
    // Forging another author's change, erasing one, or slipping in an untracked mark is refused before anything is staged, tracked
    // or not, in props, in their children and in clear.
    let opposing = r#"{"author":"Opposing","id":"90","date":"2026-01-01T00:00:00Z"}"#;
    let cases = [
        format!(
            r#"{{"op":"set","ref":"p4","props":[{{"name":"pPrChange","attrs":{opposing},"children":[{{"name":"pPr"}}]}}]}}"#
        ),
        format!(
            r#"{{"op":"set","ref":"p4","find":"thirty","props":[{{"name":"rPrChange","attrs":{opposing}}}]}}"#
        ),
        format!(
            r#"{{"op":"set","ref":"p4","target":"mark","props":[{{"name":"del","attrs":{opposing}}}]}}"#
        ),
        format!(
            r#"{{"op":"set","ref":"p4","props":[{{"name":"pBdr","children":[{{"name":"w:ins","attrs":{opposing}}}]}}]}}"#
        ),
        r#"{"op":"set","ref":"p4","target":"mark","props":[{"name":"b"}],"clear":["ins"]}"#.to_string(),
        r#"{"op":"set","ref":"p4","props":[{"name":"keepNext"}],"clear":["pPrChange"]}"#.to_string(),
        r#"{"op":"set","ref":"t1","props":[{"name":"tblGridChange"}]}"#.to_string(),
        r#"{"op":"set","ref":"r1","cell":1,"props":[{"name":"cellMerge"}]}"#.to_string(),
        r#"{"op":"set","ref":"r1","props":[{"name":"moveFromRangeStart"}]}"#.to_string(),
        r#"{"op":"set","target":"section","section":1,"props":[{"name":"titlePg"}],"clear":["sectPrChange"]}"#
            .to_string(),
        r#"{"op":"paragraph","ref":"p4","clear":["pPrChange"]}"#.to_string(),
        r#"{"op":"format","ref":"p4","clear":["rPrChange"]}"#.to_string(),
        r#"{"op":"set","ref":"p4","props":[{"name":"rPr","children":[{"name":"b"}]}]}"#.to_string(),
    ];
    for tracked in [true, false] {
        for ops in &cases {
            let mut doc = rich();
            let before = (doc.base(), doc.save().unwrap().bytes);
            let b = batch(&doc, ops);
            let edit = if tracked {
                Edit::ops(b, stamp())
            } else {
                Edit::ops(b, stamp()).untracked()
            };
            let e = doc
                .edit(edit)
                .err()
                .unwrap_or_else(|| panic!("{ops} was applied"));
            assert_eq!(e.code(), ErrorCode::InvalidPayload, "{ops}: {e}");
            assert_eq!(e.op_index(), Some(0), "{ops}");
            assert_eq!((doc.base(), doc.save().unwrap().bytes), before, "{ops}");
        }
    }
}

#[test]
fn a_section_break_through_set_is_untracked_only() {
    let ops = r#"{"op":"set","ref":"p4","props":[{"name":"sectPr","children":[{"name":"type","val":"continuous"}]}]}"#;
    let mut doc = rich();
    let e = fails(&mut doc, ops);
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(e.message().contains("only by an untracked edit"), "{e}");
    let e = fails(
        &mut doc,
        r#"{"op":"set","ref":"p4","props":[{"name":"keepNext"}],"clear":["sectPr"]}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    // Untracked, it is written, and the document has two sections.
    let b = batch(&doc, ops);
    doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(
        main_xml(&doc).contains(r#"<w:type w:val="continuous"/>"#),
        "{}",
        main_xml(&doc)
    );
    assert!(doc.check().ok, "{:?}", doc.check());
    // The header references of a section are not in what its tracked change keeps.
    let e = fails(
        &mut doc,
        r#"{"op":"set","target":"section","section":2,"props":[{"name":"headerReference","attrs":{"type":"default","r:id":"rId9"}}]}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
}

#[test]
fn a_property_the_container_does_not_allow_is_refused_wherever_the_paragraph_sits() {
    // The last paragraph sits in a body-level move, where the schema describes no paragraph: the name is still checked.
    let body = r#"<w:p><w:r><w:t>One.</w:t></w:r></w:p><w:moveFrom w:id="1" w:author="A" w:date="2025-06-01T12:00:00Z"><w:p><w:r><w:delText>Two.</w:delText></w:r></w:p></w:moveFrom>"#;
    let mut doc = Document::open(Pkg::new(body).bytes(), &OpenOptions::default()).unwrap();
    for at in ["p1", "p2"] {
        let e = fails(
            &mut doc,
            &format!(r#"{{"op":"set","ref":"{at}","props":[{{"name":"notARealProperty"}}]}}"#),
        );
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{at}: {e}");
        assert!(e.message().contains("not allowed in pPr"), "{at}: {e}");
    }
    let e = fails(
        &mut doc,
        r#"{"op":"set","ref":"p1","target":"run","props":[{"name":"jc","val":"center"}]}"#,
    );
    assert!(e.message().contains("not allowed in rPr"), "{e}");
}

#[test]
fn a_paragraph_the_schema_does_not_place_is_still_checked_by_its_own_content_model() {
    // A paragraph in a body-level move, and one under an element of an unknown namespace, with their w:pPr after a run: the
    // schema names neither paragraph where it stands, and check still sees the order.
    let body = r#"<w:p><w:r><w:t>One.</w:t></w:r></w:p><w:moveFrom w:id="1" w:author="A" w:date="2025-06-01T12:00:00Z"><w:p><w:r><w:delText>Two.</w:delText></w:r><w:pPr><w:jc w:val="center"/></w:pPr></w:p></w:moveFrom>"#;
    let doc = Document::open(Pkg::new(body).bytes(), &OpenOptions::default()).unwrap();
    let report = doc.check();
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == FindingCode::SchemaOutOfOrder && f.block.as_deref() == Some("p2")),
        "{report:?}"
    );
    // A run under an element of an unknown namespace, with its w:rPr after its text: still checked by a run's content model.
    let body = r#"<w:p><q:a xmlns:q="urn:unknown"><w:r><w:t>x</w:t><w:rPr><w:b/></w:rPr></w:r></q:a></w:p>"#;
    let doc = Document::open(Pkg::new(body).bytes(), &OpenOptions::default()).unwrap();
    let report = doc.check();
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == FindingCode::SchemaOutOfOrder),
        "{report:?}"
    );
    // A property set there now goes where the content model puts it, first in the paragraph, and adds nothing. On the side a
    // move goes away from the paragraph is gone in Final and is not edited; on the side it arrives at it is.
    let mut doc = Document::open(
        Pkg::new(r#"<w:p><w:r><w:t>One.</w:t></w:r></w:p><w:moveFrom w:id="1" w:author="A" w:date="2025-06-01T12:00:00Z"><w:p><w:r><w:delText>Two.</w:delText></w:r></w:p></w:moveFrom>"#).bytes(),
        &OpenOptions::default(),
    )
    .unwrap();
    let e = fails(
        &mut doc,
        r#"{"op":"set","ref":"p2","props":[{"name":"jc","val":"center"}]}"#,
    );
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{e}");
    let mut doc = Document::open(
        Pkg::new(r#"<w:p><w:r><w:t>One.</w:t></w:r></w:p><w:moveTo w:id="1" w:author="A" w:date="2025-06-01T12:00:00Z"><w:p><w:r><w:t>Two.</w:t></w:r></w:p></w:moveTo>"#).bytes(),
        &OpenOptions::default(),
    )
    .unwrap();
    run(
        &mut doc,
        r#"{"op":"set","ref":"p2","props":[{"name":"jc","val":"center"}]}"#,
    );
    let xml = main_xml(&doc);
    assert!(xml.contains("<w:p><w:pPr>"), "{xml}");
    assert!(
        doc.check()
            .findings
            .iter()
            .all(|f| f.code != FindingCode::SchemaOutOfOrder)
    );
}

#[test]
fn a_tracked_property_change_copies_a_problem_of_the_old_properties_without_adding_it() {
    // A second w:tblLook in the table's properties, and a second w:jc in the paragraph's. The record of a tracked change keeps
    // a copy of the old properties, problem included: the problem is the same one, not a new one.
    let look = r#"<w:tblLook w:val="04A0" w:firstRow="1" w:lastRow="0" w:firstColumn="1" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/>"#;
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="X"/><w:tblW w:type="auto" w:w="0"/>{look}{look}</w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>A</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:pPr><w:jc w:val="left"/><w:jc w:val="right"/></w:pPr><w:r><w:t>B</w:t></w:r></w:p>"#
    );
    let mut doc = Document::open(Pkg::new(&body).bytes(), &OpenOptions::default()).unwrap();
    assert_eq!(doc.check().violations, 2, "{:?}", doc.check());
    let before = main_xml(&doc);
    run(
        &mut doc,
        r#"{"op":"set","ref":"t1","props":[{"name":"jc","val":"center"}]},
           {"op":"paragraph","ref":"p2","keep_next":true}"#,
    );
    let xml = main_xml(&doc);
    for want in ["w:tblPrChange", "w:pPrChange", r#"<w:jc w:val="center"/>"#] {
        assert!(xml.contains(want), "{want} in {xml}");
    }
    reject_all(&mut doc);
    assert_eq!(
        main_xml(&doc).matches("w:tblLook").count(),
        before.matches("w:tblLook").count()
    );
}
