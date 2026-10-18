//! Tests of the fingerprints, story discovery and the id readers: known answers from hand-written
//! XML, one rule at a time.

use super::testing::{document, parse, part};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const W14_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

fn scan_body(body: &str) -> (SpanTree, Story) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (tree, story)
}

// ---- fingerprints ---------------------------------------------------------------------------

#[test]
fn fingerprints_have_known_answers() {
    let (tree, s) = scan_body("<w:p><w:r><w:t>Hello</w:t></w:r></w:p>");
    assert_eq!(s.fingerprints(&tree), [0xab6b_67d8_9d52_868c]);
    let (tree, s) = scan_body("<w:p/>");
    assert_eq!(s.fingerprints(&tree), [0xaf63_bc4c_8601_b62c]);
    let (tree, s) = scan_body("<w:p><w:r><w:t>a&amp;b</w:t></w:r></w:p>");
    assert_eq!(s.fingerprints(&tree), [0xcb5c_5f77_e9cb_dd53]);
    // runs do not matter, only the text nodes below the paragraph, in order
    let (tree, s) = scan_body("<w:p><w:r><w:t>Hello</w:t></w:r><w:r><w:t>World</w:t></w:r></w:p>");
    assert_eq!(s.fingerprints(&tree), [0x54ba_1ea0_d156_6e78]);
    // a table and a row hash the fingerprints of their children
    let (tree, s) = scan_body(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    assert_eq!(
        s.fingerprints(&tree),
        [
            0x1f66_6b90_2947_1b3c,
            0xe72a_3019_1ed8_01f6,
            0xab6b_67d8_9d52_868c
        ]
    );
    let (tree, s) = scan_body(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Hello</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    assert_eq!(
        s.fingerprints(&tree)[..2],
        [0x36f4_a703_e764_8f0b, 0xd380_63da_d273_334c]
    );
}

#[test]
fn a_fingerprint_counts_whitespace_nodes_deleted_text_and_nothing_of_formatting() {
    let fp = |body: &str| {
        let (tree, s) = scan_body(body);
        s.fingerprints(&tree)
    };
    let plain = fp("<w:p><w:r><w:t>Hello</w:t></w:r></w:p>");
    let formatted = fp(
        "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:rPr><w:b/></w:rPr><w:t>Hello</w:t></w:r></w:p>",
    );
    assert_eq!(plain, formatted, "formatting is not text");
    let spaced = fp("<w:p> <w:r><w:t>Hello</w:t></w:r></w:p>");
    assert_ne!(plain, spaced, "a whitespace-only node is a text node");
    let deleted = fp("<w:p><w:del><w:r><w:delText>Hello</w:delText></w:r></w:del></w:p>");
    assert_eq!(plain, deleted, "delText counts as text");
    let code = fp("<w:p><w:r><w:instrText>PAGE</w:instrText></w:r></w:p>");
    assert_ne!(code, fp("<w:p/>"), "field code counts as text");
}

#[test]
fn a_fingerprint_is_independent_of_the_opaque_entries() {
    let with = scan_body("<w:p><w:r><w:t>x</w:t></w:r></w:p><w:altChunk r:id=\"r\"/>");
    let without = scan_body("<w:p><w:r><w:t>x</w:t></w:r></w:p>");
    assert_eq!(
        with.1.fingerprints(&with.0),
        without.1.fingerprints(&without.0)
    );
}

// ---- stories() ------------------------------------------------------------------------------

pub(super) fn rel(id: &str, ty: &str, target: RelTarget) -> Rel {
    Rel {
        id: id.into(),
        rel_type: ty.into(),
        target,
    }
}

pub(super) const T_BASE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const S_BASE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

fn part_of(name: &str) -> RelTarget {
    RelTarget::Part(name.to_string())
}

#[test]
fn stories_come_in_a_fixed_order_with_each_part_once() {
    let main = document(
        "<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/><w:footerReference w:type=\"default\" r:id=\"rId2\"/></w:sectPr></w:pPr></w:p>\
         <w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/><w:headerReference w:type=\"first\" r:id=\"rId3\"/>\
         <w:footerReference w:type=\"even\" r:id=\"rId4\"/><w:footerReference w:type=\"first\" r:id=\"rIdNone\"/></w:sectPr>",
    );
    let rels = Rels::new(vec![
        rel(
            "rId9",
            &format!("{T_BASE}header"),
            part_of("word/header9.xml"),
        ),
        rel(
            "rId1",
            &format!("{T_BASE}header"),
            part_of("word/header1.xml"),
        ),
        rel(
            "rId2",
            &format!("{T_BASE}footer"),
            part_of("word/footer1.xml"),
        ),
        rel(
            "rId3",
            &format!("{T_BASE}footer"),
            part_of("word/header2.xml"),
        ),
        rel(
            "rId4",
            &format!("{T_BASE}footer"),
            RelTarget::External("http://x".into()),
        ),
        rel("rId5", &format!("{T_BASE}comments"), RelTarget::Invalid),
        rel(
            "rId6",
            &format!("{S_BASE}endnotes"),
            part_of("word/endnotes.xml"),
        ),
        rel(
            "rId7",
            &format!("{T_BASE}footnotes"),
            part_of("word/footnotes.xml"),
        ),
        rel(
            "rId8",
            &format!("{T_BASE}comments"),
            part_of("word/comments.xml"),
        ),
    ]);
    // from the body's own story, with no second scan, the list is the same
    let body = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &main).unwrap();
    assert_eq!(
        stories_of(&body, &main, &rels),
        stories("word/document.xml", &main, &rels)
    );
    let got: Vec<(String, StoryKind)> = stories("word/document.xml", &main, &rels)
        .into_iter()
        .map(|s| (s.part.to_string(), s.kind))
        .collect();
    assert_eq!(
        got,
        [
            ("word/document.xml".to_string(), StoryKind::Body),
            ("word/header1.xml".to_string(), StoryKind::Header),
            ("word/footer1.xml".to_string(), StoryKind::Footer),
            ("word/footnotes.xml".to_string(), StoryKind::Footnotes),
            ("word/endnotes.xml".to_string(), StoryKind::Endnotes),
            ("word/comments.xml".to_string(), StoryKind::Comments),
        ],
        "header9 is not named by a sectPr, header2 is named through a relationship of another type, \
         rId4 is external, rIdNone does not exist, the first comments relationship is invalid"
    );
}

#[test]
fn a_main_part_that_is_not_a_document_still_lists_the_body_first() {
    let tree = part("word/document.xml", "hdr", "");
    let got = stories("word/document.xml", &tree, &Rels::default());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].kind, StoryKind::Body);
}

// ---- existing_ids ---------------------------------------------------------------------------

fn ids(kind: IdKind, tree: &SpanTree) -> Vec<u32> {
    let mut out = Vec::new();
    existing_ids(kind, tree, &mut out);
    out
}

#[test]
fn paragraph_and_text_ids_are_found_on_any_element_under_any_prefix() {
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_NS}\" xmlns:zz=\"{W14_NS}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\">\
             <w:body><w:p zz:paraId=\"0000ABCD\" zz:textId=\"77777777\"/><w:p zz:paraId=\"1234567\"/><w:p zz:paraId=\"123456789\"/>\
             <w:p zz:paraId=\"+000ABCD\"/><w:p zz:paraId=\"0000GHIJ\"/><w:tbl><w:tr zz:paraId=\"DEADBEEF\" zz:textId=\"0000000A\"/></w:tbl>\
             <w:p paraId=\"00000009\"/>\
             <mc:AlternateContent><mc:Choice Requires=\"zz\"><w:p zz:paraId=\"00000001\"/></mc:Choice><mc:Fallback><w:p zz:paraId=\"00000002\"/></mc:Fallback></mc:AlternateContent>\
             </w:body></w:document>"
        ),
    );
    assert_eq!(
        ids(IdKind::ParaId, &tree),
        [0xABCD, 0xDEAD_BEEF, 1, 2],
        "both branches; bad values, a leading plus and no namespace skipped"
    );
    assert_eq!(ids(IdKind::TextId, &tree), [0x7777_7777, 0xA]);
}

#[test]
fn comment_parts_name_paragraphs_and_durable_ids() {
    let ex = parse(
        "word/commentsExtended.xml",
        "<w15:commentsEx xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\"><w15:commentEx w15:paraId=\"000000A1\" w15:done=\"0\"/></w15:commentsEx>",
    );
    assert_eq!(ids(IdKind::ParaId, &ex), [0xA1]);
    // Word 2013's preview wrote `w15` in its 2010/11 namespace (`Comment042`, `Invalid_Word15Comments`).
    let preview = parse(
        "word/commentsExtended.xml",
        "<w15:commentsEx xmlns:w15=\"http://schemas.microsoft.com/office/word/2010/11/wordml\"><w15:commentEx w15:paraId=\"000000A2\" w15:paraIdParent=\"000000A1\" w15:done=\"0\"/></w15:commentsEx>",
    );
    assert_eq!(ids(IdKind::ParaId, &preview), [0xA2]);
    let cid = parse(
        "word/commentsIds.xml",
        "<w16cid:commentsIds xmlns:w16cid=\"http://schemas.microsoft.com/office/word/2016/wordml/cid\"><w16cid:commentId w16cid:paraId=\"000000B1\" w16cid:durableId=\"000000C1\"/></w16cid:commentsIds>",
    );
    assert_eq!(ids(IdKind::ParaId, &cid), [0xB1]);
    assert_eq!(ids(IdKind::CommentDurableId, &cid), [0xC1]);
    let cex = parse(
        "word/commentsExtensible.xml",
        "<w16cex:commentsExtensible xmlns:w16cex=\"http://schemas.microsoft.com/office/word/2018/wordml/cex\"><w16cex:commentExtensible w16cex:durableId=\"000000D1\"/></w16cex:commentsExtensible>",
    );
    assert_eq!(ids(IdKind::CommentDurableId, &cex), [0xD1]);
    let numbering = parse(
        "word/numbering.xml",
        &format!(
            "<w:numbering xmlns:w=\"{W_NS}\" xmlns:w16cid=\"http://schemas.microsoft.com/office/word/2016/wordml/cid\"><w:num w:numId=\"1\" w16cid:durableId=\"000000E1\"/></w:numbering>"
        ),
    );
    assert!(
        ids(IdKind::CommentDurableId, &numbering).is_empty(),
        "a numbering durable id is another id space"
    );
}

#[test]
fn annotation_ids_are_the_non_negative_decimal_w_ids() {
    let tree = document(
        "<w:p><w:ins w:id=\"5\" w:author=\"a\"/><w:bookmarkStart w:id=\"7\" w:name=\"n\"/><w:del w:id=\"-1\"/><w:del w:id=\"x\"/>\
         <w:commentRangeStart w:id=\"4294967295\"/><w:commentRangeStart w:id=\"4294967296\"/></w:p>",
    );
    assert_eq!(ids(IdKind::AnnotationId, &tree), [5, 7, u32::MAX]);
}

#[test]
fn id_parts_name_the_parts_to_read_for_each_kind() {
    let specs = vec![
        StorySpec {
            part: Arc::from("word/document.xml"),
            kind: StoryKind::Body,
        },
        StorySpec {
            part: Arc::from("word/header1.xml"),
            kind: StoryKind::Header,
        },
        StorySpec {
            part: Arc::from("word/comments.xml"),
            kind: StoryKind::Comments,
        },
    ];
    let ms = "http://schemas.microsoft.com/office/";
    let rels = Rels::new(vec![
        rel(
            "a",
            &format!("{ms}2011/relationships/commentsExtended"),
            part_of("word/commentsExtended.xml"),
        ),
        rel(
            "b",
            &format!("{ms}2016/09/relationships/commentsIds"),
            part_of("word/commentsIds.xml"),
        ),
        rel(
            "c",
            &format!("{ms}2018/08/relationships/commentsExtensible"),
            part_of("word/commentsExtensible.xml"),
        ),
    ]);
    let names = |k| {
        id_parts(k, &specs, &rels)
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
    };
    let stories = ["word/document.xml", "word/header1.xml", "word/comments.xml"];
    let mut para: Vec<&str> = stories.to_vec();
    para.extend(["word/commentsExtended.xml", "word/commentsIds.xml"]);
    assert_eq!(names(IdKind::ParaId), para);
    assert_eq!(names(IdKind::TextId), stories);
    assert_eq!(names(IdKind::AnnotationId), stories);
    assert_eq!(
        names(IdKind::CommentDurableId),
        ["word/commentsIds.xml", "word/commentsExtensible.xml"]
    );
    assert_eq!(
        id_parts(IdKind::CommentDurableId, &specs, &Rels::default()).len(),
        0
    );
}
