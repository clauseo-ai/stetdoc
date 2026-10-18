//! The Word rules that are read inside a unit, one fixture each: a fragment, and the findings it
//! must give, by code and subject. The fixtures are hand-written, and what the corpus says about
//! the same rules is in `tests_corpus`.

use super::keys::Key;
use super::tests::{W, document_xml, named, tree};
use super::{Found, scan_part, scan_unit};
use crate::api::check::FindingCode as C;

const DEL: &str = r#"w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z""#;

/// The code and subject of each finding, in document order.
fn read(found: &[Found]) -> Vec<(C, String)> {
    found
        .iter()
        .map(|f| match &f.key {
            Key::Rule { code, subject } => (*code, subject.clone()),
            Key::Schema(k) => panic!("a schema key in a Word rule: {k:?}"),
        })
        .collect()
}

fn expected(list: &[(C, &str)]) -> Vec<(C, String)> {
    list.iter().map(|(c, s)| (*c, (*s).to_string())).collect()
}

/// The findings of a body.
fn body(xml: &str) -> Vec<(C, String)> {
    read(&scan_part(&tree(&document_xml(xml))))
}

#[track_caller]
fn case(label: &str, xml: &str, want: &[(C, &str)]) {
    assert_eq!(body(xml), expected(want), "{label}: {xml}");
}

#[test]
fn text_belongs_in_w_t_outside_a_deletion_and_in_w_del_text_inside_one() {
    case(
        "ok",
        &format!(
            r#"<w:p><w:del {DEL}><w:r><w:delText>x</w:delText></w:r></w:del><w:r><w:t>y</w:t></w:r></w:p>"#
        ),
        &[],
    );
    case(
        "t in a deletion",
        &format!(r#"<w:p><w:del {DEL}><w:r><w:t>x</w:t></w:r></w:del></w:p>"#),
        &[(C::TextInDeletion, "t")],
    );
    case(
        "instruction text in a deletion",
        &format!(
            r#"<w:p><w:del {DEL}><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r></w:del></w:p>"#
        ),
        &[(C::TextInDeletion, "instrText")],
    );
    case(
        "delText outside",
        r#"<w:p><w:r><w:delText>x</w:delText></w:r></w:p>"#,
        &[(C::DeltextOutsideDeletion, "delText")],
    );
    case(
        "delInstrText outside",
        r#"<w:p><w:r><w:delInstrText>x</w:delInstrText></w:r></w:p>"#,
        &[(C::DeltextOutsideDeletion, "delInstrText")],
    );
    // text that moved away stays w:t
    case(
        "moveFrom",
        &format!(r#"<w:p><w:moveFrom {DEL}><w:r><w:t>x</w:t></w:r></w:moveFrom></w:p>"#),
        &[],
    );
    // a deletion belongs to its paragraph
    case(
        "another paragraph",
        &format!(
            r#"<w:p><w:del {DEL}><w:r><w:delText>a</w:delText></w:r></w:del></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p>"#
        ),
        &[],
    );
    // and so does the paragraph of a text box inside a deleted run
    case(
        "text box in a deletion",
        &format!(
            r#"<w:p><w:del {DEL}><w:r><w:pict><w:txbxContent><w:p><w:r><w:t>in</w:t></w:r></w:p></w:txbxContent></w:pict></w:r></w:del></w:p>"#
        ),
        &[],
    );
    case(
        "a deletion in a text box",
        &format!(
            r#"<w:p><w:r><w:pict><w:txbxContent><w:p><w:del {DEL}><w:r><w:t>in</w:t></w:r></w:del></w:p></w:txbxContent></w:pict></w:r></w:p>"#
        ),
        &[(C::TextInDeletion, "t")],
    );
    // a deletion around a whole paragraph is not where the schema puts one (stemma's
    // `block-level-del`), and the rule is "within one paragraph", so it says nothing of the text
    case(
        "a deletion around a paragraph",
        &format!(r#"<w:del {DEL}><w:p><w:r><w:t>whole</w:t></w:r></w:p></w:del>"#),
        &[],
    );
}

#[test]
fn text_with_white_space_at_an_edge_needs_xml_space() {
    let t = |text: &str| format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>");
    case("leading", &t(" a"), &[(C::EdgeSpaceWithoutPreserve, "t")]);
    case("trailing", &t("a "), &[(C::EdgeSpaceWithoutPreserve, "t")]);
    case("tab", &t("a\t"), &[(C::EdgeSpaceWithoutPreserve, "t")]);
    case(
        "line feed",
        &t("\na"),
        &[(C::EdgeSpaceWithoutPreserve, "t")],
    );
    case("inside", &t("a b"), &[]);
    case(
        "a no-break space is not XML white space",
        &t("\u{a0}a\u{a0}"),
        &[],
    );
    case("empty", &t(""), &[]);
    case(
        "preserve",
        r#"<w:p><w:r><w:t xml:space="preserve"> a </w:t></w:r></w:p>"#,
        &[],
    );
    case(
        "default is not preserve",
        r#"<w:p><w:r><w:t xml:space="default"> a</w:t></w:r></w:p>"#,
        &[(C::EdgeSpaceWithoutPreserve, "t")],
    );
    case(
        "character data",
        r#"<w:p><w:r><w:t><![CDATA[ a]]></w:t></w:r></w:p>"#,
        &[(C::EdgeSpaceWithoutPreserve, "t")],
    );
    case(
        "deleted text",
        &format!(r#"<w:p><w:del {DEL}><w:r><w:delText> a</w:delText></w:r></w:del></w:p>"#),
        // the key names the element as it was before the compiler renamed it
        &[(C::EdgeSpaceWithoutPreserve, "t")],
    );
    case(
        "instruction text",
        r#"<w:p><w:r><w:instrText> PAGE </w:instrText></w:r></w:p>"#,
        &[(C::EdgeSpaceWithoutPreserve, "instrText")],
    );
}

#[test]
fn a_revision_is_not_nested_in_a_revision_of_its_kind() {
    let ins = |inner: &str| format!(r#"<w:p><w:ins {DEL}>{inner}</w:ins></w:p>"#);
    case(
        "ins in ins",
        &ins(&format!(r#"<w:ins {DEL}><w:r><w:t>x</w:t></w:r></w:ins>"#)),
        &[(C::RevisionNestedInItself, "ins")],
    );
    case(
        "del in del",
        &format!(
            r#"<w:p><w:del {DEL}><w:del {DEL}><w:r><w:delText>x</w:delText></w:r></w:del></w:del></w:p>"#
        ),
        &[(C::RevisionNestedInItself, "del")],
    );
    // another author's deletion inside an insertion is what Word 16 writes
    case(
        "del in ins",
        &ins(&format!(
            r#"<w:del {DEL}><w:r><w:delText>x</w:delText></w:r></w:del>"#
        )),
        &[],
    );
    // the mark's own revision elements are names, and are not nested in anything
    case(
        "the mark of a paragraph",
        &format!(
            r#"<w:p><w:pPr><w:rPr><w:ins {DEL}/></w:rPr></w:pPr><w:ins {DEL}><w:r><w:t>x</w:t></w:r></w:ins></w:p>"#
        ),
        &[],
    );
    // a revision in a paragraph of a text box is not in the paragraph around it
    case(
        "a text box",
        &ins(&format!(
            r#"<w:r><w:pict><w:txbxContent><w:p><w:ins {DEL}><w:r><w:t>x</w:t></w:r></w:ins></w:p></w:txbxContent></w:pict></w:r>"#
        )),
        &[],
    );
}

#[test]
fn a_table_cell_ends_with_a_paragraph_and_a_table_holds_rows() {
    let cell = |content: &str| {
        format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc>{content}</w:tc></w:tr></w:tbl>"#
        )
    };
    let bad = &[(C::CellWithoutFinalParagraph, "tc")];
    case("a paragraph", &cell("<w:p/>"), &[]);
    case("empty", &cell(""), bad);
    case("only properties", &cell("<w:tcPr/>"), bad);
    case(
        "a table last",
        &cell("<w:p/><w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"),
        bad,
    );
    case(
        "a marker after the paragraph",
        &cell(r#"<w:p/><w:bookmarkEnd w:id="1"/>"#),
        &[],
    );
    case(
        "a content control",
        &cell("<w:sdt><w:sdtPr/><w:sdtContent><w:p/></w:sdtContent></w:sdt>"),
        &[],
    );
    case(
        "a content control with a table last",
        &cell(
            "<w:sdt><w:sdtPr/><w:sdtContent><w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:sdtContent></w:sdt>",
        ),
        bad,
    );
    case(
        "an empty content control",
        &cell("<w:sdt><w:sdtPr/></w:sdt>"),
        bad,
    );
    case(
        "custom XML",
        &cell("<w:customXml w:element=\"a\"><w:customXmlPr/><w:p/></w:customXml>"),
        &[],
    );
    case(
        "no row",
        "<w:tbl><w:tblPr/><w:tblGrid/></w:tbl>",
        &[(C::TableWithoutRow, "tbl")],
    );
    case(
        "a content control of rows",
        "<w:tbl><w:tblPr/><w:tblGrid/><w:sdt><w:sdtContent><w:tr><w:tc><w:p/></w:tc></w:tr></w:sdtContent></w:sdt></w:tbl>",
        &[],
    );
    case(
        "no cell",
        r#"<w:tbl><w:tblPr/><w:tblGrid/><w:tr/></w:tbl>"#,
        &[(C::RowWithoutCell, "tr")],
    );
}

#[test]
fn a_note_or_a_comment_holds_a_block() {
    let part = |root: &str, item: &str, inner: &str| {
        format!(r#"<w:{root} xmlns:w="{W}"><w:{item} w:id="1">{inner}</w:{item}></w:{root}>"#)
    };
    let read_part = |xml: &str| read(&scan_part(&tree(xml)));
    for (root, item) in [
        ("footnotes", "footnote"),
        ("endnotes", "endnote"),
        ("comments", "comment"),
    ] {
        assert_eq!(
            read_part(&part(root, item, "")),
            expected(&[(C::ContainerWithoutBlock, item)])
        );
        assert_eq!(read_part(&part(root, item, "<w:p/>")), expected(&[]));
        assert_eq!(
            read_part(&part(
                root,
                item,
                "<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"
            )),
            expected(&[])
        );
    }
    // the separator references in the properties of a document are not notes
    let settings = format!(
        r#"<w:settings xmlns:w="{W}"><w:footnotePr><w:footnote w:id="-1"/></w:footnotePr></w:settings>"#
    );
    assert_eq!(read_part(&settings), expected(&[]));
}

#[test]
fn a_revision_or_a_property_container_holds_something() {
    case(
        "empty insertion",
        &format!(r#"<w:p><w:ins {DEL}/></w:p>"#),
        &[(C::EmptyRevision, "ins")],
    );
    case(
        "empty deletion",
        &format!(r#"<w:p><w:del {DEL}/></w:p>"#),
        &[(C::EmptyRevision, "del")],
    );
    case(
        "empty move",
        &format!(r#"<w:p><w:moveFrom {DEL}/><w:moveTo {DEL}/></w:p>"#),
        &[(C::EmptyRevision, "moveFrom"), (C::EmptyRevision, "moveTo")],
    );
    case(
        "an insertion with a run",
        &format!(r#"<w:p><w:ins {DEL}><w:r/></w:ins></w:p>"#),
        &[],
    );
    case(
        "empty paragraph properties",
        "<w:p><w:pPr/></w:p>",
        &[(C::EmptyProperties, "pPr")],
    );
    case(
        "empty run properties",
        "<w:p><w:r><w:rPr/><w:t>x</w:t></w:r></w:p>",
        &[(C::EmptyProperties, "rPr")],
    );
    case(
        "empty mark properties",
        "<w:p><w:pPr><w:rPr/></w:pPr></w:p>",
        &[(C::EmptyProperties, "rPr")],
    );
    case(
        "the mark of a row",
        &format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:trPr><w:ins {DEL}/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#
        ),
        &[],
    );
    // the old properties of a property change may be empty, and Word writes them so
    case(
        "an empty snapshot",
        &format!(
            r#"<w:p><w:pPr><w:pStyle w:val="A"/><w:pPrChange {DEL}><w:pPr/></w:pPrChange></w:pPr><w:r><w:rPr><w:b/><w:rPrChange {DEL}><w:rPr/></w:rPrChange></w:rPr><w:t>x</w:t></w:r></w:p>"#
        ),
        &[],
    );
}

#[test]
fn what_word_does_not_read_is_not_read() {
    // a deletion with text in it, in the fallback of a branch that is not taken
    let alternate = |requires: &str| {
        format!(
            r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="{requires}"><w:t>choice</w:t></mc:Choice><mc:Fallback><w:del {DEL}><w:r><w:t> fallback</w:t></w:r></w:del></mc:Fallback></mc:AlternateContent></w:r></w:p>"#
        )
    };
    case("the choice is read", &alternate("wps"), &[]);
    assert_eq!(
        body(&alternate("x")),
        expected(&[(C::TextInDeletion, "t"), (C::EdgeSpaceWithoutPreserve, "t")]),
        "the fallback is read when the choice is not"
    );
    // an element that the document says to ignore, with everything in it
    case(
        "an ignorable element",
        r#"<w:p mc:Ignorable="x"><x:note><w:r><w:rPr/><w:t> a</w:t></w:r></x:note></w:p>"#,
        &[],
    );
}

#[test]
fn the_strict_namespace_is_read_like_the_other() {
    let strict = r#"<w:document xmlns:w="http://purl.oclc.org/ooxml/wordprocessingml/main"><w:body><w:p><w:del w:id="1" w:author="a"><w:r><w:t> x</w:t></w:r></w:del></w:p></w:body></w:document>"#;
    assert_eq!(
        read(&scan_part(&tree(strict))),
        expected(&[(C::TextInDeletion, "t"), (C::EdgeSpaceWithoutPreserve, "t")])
    );
}

#[test]
fn a_unit_is_read_without_the_paragraphs_of_its_text_boxes() {
    let xml = document_xml(&format!(
        r#"<w:p><w:r><w:rPr/></w:r><w:r><w:pict><w:txbxContent><w:p><w:pPr/><w:del {DEL}><w:r><w:t>in</w:t></w:r></w:del></w:p></w:txbxContent></w:pict></w:r></w:p>"#
    ));
    let t = tree(&xml);
    let paragraphs = named(&t, "p");
    let (outer, inner) = (paragraphs[0], paragraphs[1]);
    assert_eq!(
        read(&scan_unit(&t, outer)),
        expected(&[(C::EmptyProperties, "rPr")])
    );
    assert_eq!(
        read(&scan_unit(&t, inner)),
        expected(&[(C::EmptyProperties, "pPr"), (C::TextInDeletion, "t")])
    );
    // a whole part reads both, and each once
    assert_eq!(
        read(&scan_part(&t)),
        expected(&[
            (C::EmptyProperties, "rPr"),
            (C::EmptyProperties, "pPr"),
            (C::TextInDeletion, "t"),
        ])
    );
    // every finding names the element it is about
    for found in scan_part(&t) {
        assert!(found.node.is_some());
        assert!(!found.message.is_empty());
    }
}

#[test]
fn a_finding_has_a_key_that_names_no_place() {
    let findings = scan_part(&tree(&document_xml(
        r#"<w:p><w:pPr/></w:p><w:p><w:pPr/></w:p>"#,
    )));
    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].key, findings[1].key);
    assert_ne!(findings[0].node, findings[1].node);
}
