//! Fingerprints (`docs/design/wordml.md` 3.4) and story discovery (3.1): the answers of a small fixed
//! document written down once and never edited, an independent computation of the same values, and
//! `stories()` against relationships whose type does not match what names them.

use super::testing::document;
use super::tests_gen::Gen;
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

/// FNV-1a 64, written again here so the known answers do not rest on the code under test.
fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

const START: u64 = 0xcbf2_9ce4_8422_2325;

/// The fixed document: every kind of block, a nested table, deleted text, a reference, an astral
/// character and CDATA.
const FIXED: &str = "<w:p><w:r><w:t>Alpha</w:t></w:r></w:p>\
<w:tbl><w:tr><w:tc><w:p><w:r><w:t>Beta</w:t></w:r></w:p><w:p/></w:tc>\
<w:tc><w:tbl><w:tr><w:tc><w:p><w:del w:id=\"1\"><w:r><w:delText>Gamma</w:delText></w:r></w:del></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>\
<w:p><w:r><w:t>caf&#xe9; &amp; \u{1F600}</w:t></w:r><w:r><w:t><![CDATA[x<y]]></w:t></w:r></w:p>";

fn fingerprints(body: &str) -> Vec<u64> {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    story.fingerprints(&tree)
}

#[test]
fn the_fingerprint_of_every_block_of_a_fixed_document_is_frozen() {
    // These values are the frozen format of FINGERPRINT_VERSION 1 (3.4). They must never be edited:
    // a change here is a change of the format, which needs a new version and is a major release.
    // Order: Alpha, the outer table, its row, Beta, the empty paragraph, the inner table, its row,
    // Gamma, the last paragraph.
    const FROZEN: [u64; 9] = [
        0x5345_16be_1856_cc30,
        0x1158_4a11_67a7_c4ae,
        0x2b78_38b0_fcf5_3918,
        0x00e4_b4d3_21f0_6ede,
        0xaf63_bc4c_8601_b62c,
        0x95e5_2be3_d9e7_b667,
        0xfe0c_d83b_c8db_4f3d,
        0x29f5_452b_0b03_d22d,
        0xc687_ff48_3ed9_4ce0,
    ];
    assert_eq!(FINGERPRINT_VERSION, 1);
    assert_eq!(fingerprints(FIXED), FROZEN);
}

/// A paragraph's fingerprint, written from the definition: kind byte 1, then its text.
fn paragraph(text: &str) -> u64 {
    fnv(fnv(START, &[1]), text.as_bytes())
}

/// A table or a row: its kind byte, then the 8 little-endian bytes of each child.
fn parent(kind: u8, children: &[u64]) -> u64 {
    let mut hash = fnv(START, &[kind]);
    for c in children {
        hash = fnv(hash, &c.to_le_bytes());
    }
    hash
}

#[test]
fn the_frozen_values_are_what_the_definition_gives_for_the_text_of_the_document() {
    let (gamma, empty) = (paragraph("Gamma"), paragraph(""));
    let inner = parent(2, &[parent(3, &[gamma])]);
    let outer_row = parent(3, &[paragraph("Beta"), empty, inner]);
    let want = [
        paragraph("Alpha"),
        parent(2, &[outer_row]),
        outer_row,
        paragraph("Beta"),
        empty,
        inner,
        parent(3, &[gamma]),
        gamma,
        paragraph("caf\u{e9} & \u{1F600}x<y"),
    ];
    assert_eq!(fingerprints(FIXED), want);
}

/// The fingerprints of a body worked out from the tree alone: a paragraph hashes all its text
/// (`text_content` reads text and CDATA in document order), a table and a row their entries.
fn oracle(tree: &SpanTree, story: &Story) -> Vec<u64> {
    let entries = story.blocks();
    let mut out = vec![0u64; entries.len()];
    for i in (0..entries.len()).rev() {
        out[i] = match entries[i].kind {
            BlockKind::Paragraph => paragraph(&tree.node_ref(entries[i].node).text_content()),
            kind => {
                let kids: Vec<u64> = (0..entries.len())
                    .filter(|&k| entries[k].parent == Some(i as u32))
                    .map(|k| out[k])
                    .collect();
                parent(if kind == BlockKind::Table { 2 } else { 3 }, &kids)
            }
        };
    }
    out
}

#[test]
fn generated_documents_have_the_fingerprints_the_definition_gives() {
    for seed in 1..40 {
        let mut g = Gen::new(seed);
        let body = g.body(6).xml;
        let tree = document(&body);
        let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
        let got = story.fingerprints(&tree);
        assert_eq!(got, oracle(&tree, &story), "{body}");
        assert_eq!(got, story.fingerprints(&tree));
        // a re-parse of the same bytes, and a scan of it, give the same
        let again = document(&body);
        let scanned =
            Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &again).unwrap();
        assert_eq!(got, scanned.fingerprints(&again));
    }
}

#[test]
fn a_changed_character_changes_its_paragraph_and_the_blocks_around_it_and_nothing_else() {
    let before = fingerprints(FIXED);
    let after = fingerprints(&FIXED.replace("Gamma", "Gammb"));
    let changed: Vec<usize> = (0..9).filter(|&i| before[i] != after[i]).collect();
    // Gamma, its row, the inner table, the outer row and the outer table
    assert_eq!(changed, [1, 2, 5, 6, 7]);
    // a space in a text node is a change, and formatting is not
    let styled = FIXED.replace(
        "<w:r><w:t>Alpha",
        "<w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Alpha",
    );
    assert_eq!(fingerprints(&styled), before);
    assert_ne!(fingerprints(&FIXED.replace("Alpha", "Alpha ")), before);
}

// ---- stories() against relationships whose type does not match --------------------------------------

const T_BASE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const S_BASE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

fn rel(id: &str, ty: &str, name: &str) -> Rel {
    Rel {
        id: id.into(),
        rel_type: ty.into(),
        target: RelTarget::Part(name.into()),
    }
}

fn names(tree: &SpanTree, rels: &[Rel]) -> Vec<(String, StoryKind)> {
    stories("word/document.xml", tree, &Rels::new(rels.to_vec()))
        .into_iter()
        .map(|s| (s.part.to_string(), s.kind))
        .collect()
}

fn with_sections(sects: &str) -> SpanTree {
    document(&format!("<w:p><w:pPr>{sects}</w:pPr></w:p>"))
}

const BODY_ONLY: (&str, StoryKind) = ("word/document.xml", StoryKind::Body);

#[test]
fn a_reference_counts_only_when_the_relationship_type_matches_the_kind_of_the_reference() {
    let tree = with_sections(
        "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"a\"/><w:footerReference w:type=\"default\" r:id=\"b\"/>\
         <w:headerReference w:type=\"first\" r:id=\"c\"/><w:footerReference w:type=\"first\" r:id=\"d\"/></w:sectPr>",
    );
    let h = format!("{T_BASE}header");
    let f = format!("{T_BASE}footer");
    // every reference names a relationship of the other kind: nothing but the body is a story
    let crossed = [
        rel("a", &f, "word/f1.xml"),
        rel("b", &h, "word/h1.xml"),
        rel("c", &f, "word/f2.xml"),
        rel("d", &h, "word/h2.xml"),
    ];
    assert_eq!(
        names(&tree, &crossed),
        [BODY_ONLY.0.to_string()].map(|n| (n, StoryKind::Body))
    );
    // the matching ones count, in the order the references are written
    let right = [
        rel("a", &h, "word/h1.xml"),
        rel("b", &f, "word/f1.xml"),
        rel("c", &h, "word/h2.xml"),
        rel("d", &f, "word/f2.xml"),
    ];
    let got: Vec<String> = names(&tree, &right).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        got,
        [
            "word/document.xml",
            "word/h1.xml",
            "word/f1.xml",
            "word/h2.xml",
            "word/f2.xml"
        ]
    );
}

#[test]
fn a_type_that_only_looks_like_the_right_one_does_not_match() {
    let tree =
        with_sections("<w:sectPr><w:headerReference w:type=\"default\" r:id=\"a\"/></w:sectPr>");
    for ty in [
        format!("{T_BASE}headerx"),
        format!("{T_BASE}xheader"),
        format!("{T_BASE}Header"),
        format!("{T_BASE}header/"),
        T_BASE.to_string(),
        format!("{S_BASE}footer"),
        format!("http://example.com/{}", "header"),
        String::new(),
        "header".to_string(),
        format!("{S_BASE}{T_BASE}header"),
    ] {
        assert_eq!(
            names(&tree, &[rel("a", &ty, "word/h.xml")]),
            [(BODY_ONLY.0.to_string(), BODY_ONLY.1)],
            "{ty:?}"
        );
    }
    for ty in [format!("{T_BASE}header"), format!("{S_BASE}header")] {
        assert_eq!(
            names(&tree, &[rel("a", &ty, "word/h.xml")]).len(),
            2,
            "{ty}"
        );
    }
}

#[test]
fn a_reference_with_no_usable_relationship_is_skipped() {
    let h = format!("{T_BASE}header");
    let tree = with_sections(
        "<w:sectPr><w:headerReference w:type=\"default\"/><w:headerReference w:type=\"first\" r:id=\"ext\"/>\
         <w:headerReference w:type=\"even\" r:id=\"bad\"/><w:headerReference r:id=\"gone\"/></w:sectPr>",
    );
    let rels = [
        Rel {
            id: "ext".into(),
            rel_type: h.clone(),
            target: RelTarget::External("http://x/h.xml".into()),
        },
        Rel {
            id: "bad".into(),
            rel_type: h,
            target: RelTarget::Invalid,
        },
    ];
    assert_eq!(
        names(&tree, &rels),
        [(BODY_ONLY.0.to_string(), BODY_ONLY.1)]
    );
}

#[test]
fn one_part_named_by_both_kinds_of_reference_is_one_story_of_the_kind_its_type_says() {
    let tree = with_sections(
        "<w:sectPr><w:footerReference r:id=\"a\"/><w:headerReference r:id=\"a\"/><w:headerReference r:id=\"b\"/></w:sectPr>",
    );
    let h = format!("{T_BASE}header");
    let got = names(
        &tree,
        &[rel("a", &h, "word/x.xml"), rel("b", &h, "word/x.xml")],
    );
    assert_eq!(
        got,
        [
            (BODY_ONLY.0.to_string(), BODY_ONLY.1),
            ("word/x.xml".to_string(), StoryKind::Header)
        ]
    );
}

#[test]
fn notes_and_comments_come_from_their_own_relationship_types_only() {
    let tree = with_sections("");
    let rels = [
        rel("1", &format!("{T_BASE}footnotes"), "word/fn.xml"),
        rel("2", &format!("{T_BASE}endnotes"), "word/en.xml"),
        rel("3", &format!("{T_BASE}comments"), "word/c.xml"),
        rel("4", &format!("{T_BASE}header"), "word/unreferenced.xml"),
        rel("5", &format!("{T_BASE}commentsExtended"), "word/ce.xml"),
        rel("6", &format!("{T_BASE}footnote"), "word/nope.xml"),
    ];
    let got = names(&tree, &rels);
    assert_eq!(
        got.iter().map(|(_, k)| *k).collect::<Vec<_>>(),
        [
            StoryKind::Body,
            StoryKind::Footnotes,
            StoryKind::Endnotes,
            StoryKind::Comments
        ]
    );
    // a footnotes relationship under the endnotes type's name is not a footnotes story
    assert_eq!(
        names(
            &tree,
            &[rel("1", &format!("{T_BASE}endnotes"), "word/fn.xml")]
        )[1]
        .1,
        StoryKind::Endnotes
    );
}
