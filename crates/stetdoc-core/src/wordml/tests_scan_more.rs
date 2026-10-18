//! More tests of the block scan (`docs/design/wordml.md` 3.2) than `tests_story.rs` holds: the order
//! and parents of tables nested to depth four against a builder that knows the answer, every
//! transparent wrapper in every position it can stand in, `opaque()` in every place, notes with their
//! separators, and a part that has the wrong root.

use super::testing::{document, part, story};
use super::*;
use crate::api::error::ErrorCode;
use std::sync::Arc;

use BlockKind::{Paragraph as P, Row as R, Table as T};

type Shape = Vec<(BlockKind, Option<u32>)>;
type Wrapper = fn(&str) -> String;
type Position = fn(&dyn Fn(&str) -> String) -> String;

fn shape(s: &Story) -> Shape {
    s.blocks().iter().map(|e| (e.kind, e.parent)).collect()
}

// ---- tables nested to depth four ------------------------------------------------------------------

/// A table `depth` levels deep: two rows of two cells, and in the first cell of the first row a
/// paragraph, the next table, and a paragraph. Writes what the scan must list into `out`.
fn nested(depth: usize, parent: Option<u32>, out: &mut Shape) -> String {
    let table = out.len() as u32;
    out.push((T, parent));
    let mut xml = String::from("<w:tbl><w:tblPr/><w:tblGrid/>");
    for r in 0..2 {
        let row = out.len() as u32;
        out.push((R, Some(table)));
        xml.push_str("<w:tr><w:trPr/>");
        for c in 0..2 {
            xml.push_str("<w:tc><w:tcPr/>");
            out.push((P, Some(row)));
            xml.push_str("<w:p/>");
            if r == 0 && c == 0 && depth > 1 {
                xml.push_str(&nested(depth - 1, Some(row), out));
                out.push((P, Some(row)));
                xml.push_str("<w:p/>");
            }
            xml.push_str("</w:tc>");
        }
        xml.push_str("</w:tr>");
    }
    xml.push_str("</w:tbl>");
    xml
}

#[test]
fn tables_nested_to_depth_four_come_in_pre_order_with_the_parent_of_each() {
    for depth in 1..=4 {
        let mut want = vec![(P, None)];
        let xml = format!("<w:p/>{}<w:p/>", nested(depth, None, &mut want));
        want.push((P, None));
        let s = story(&xml);
        assert_eq!(shape(&s), want, "depth {depth}");
        assert!(s.opaque().is_empty() && s.anomalies().is_empty());
        // the parent of every entry comes before it, and is a table for a row, a row for the rest
        for (i, e) in s.blocks().iter().enumerate() {
            if let Some(p) = e.parent {
                assert!((p as usize) < i);
                let kind = s.blocks()[p as usize].kind;
                assert_eq!(kind, if e.kind == R { T } else { R });
            }
        }
    }
}

// ---- transparent containers in every position -----------------------------------------------------

/// Each wrapper that is walked through, as a function from what it holds to the XML.
fn wrappers() -> Vec<(&'static str, Wrapper)> {
    vec![
        ("sdt", |x| {
            format!("<w:sdt><w:sdtPr/><w:sdtContent>{x}</w:sdtContent></w:sdt>")
        }),
        ("customXml", |x| {
            format!("<w:customXml w:uri=\"u\" w:element=\"e\"><w:customXmlPr/>{x}</w:customXml>")
        }),
        ("smartTag", |x| {
            format!("<w:smartTag w:uri=\"u\" w:element=\"e\"><w:smartTagPr/>{x}</w:smartTag>")
        }),
        ("ins", |x| {
            format!("<w:ins w:id=\"1\" w:author=\"a\">{x}</w:ins>")
        }),
        ("del", |x| {
            format!("<w:del w:id=\"1\" w:author=\"a\">{x}</w:del>")
        }),
        ("moveFrom", |x| {
            format!("<w:moveFrom w:id=\"1\" w:author=\"a\">{x}</w:moveFrom>")
        }),
        ("moveTo", |x| {
            format!("<w:moveTo w:id=\"1\" w:author=\"a\">{x}</w:moveTo>")
        }),
        ("choice", |x| {
            format!(
                "<mc:AlternateContent><mc:Choice Requires=\"wps\">{x}</mc:Choice><mc:Fallback><w:p><w:r><w:t>fallback</w:t></w:r></w:p></mc:Fallback></mc:AlternateContent>"
            )
        }),
        ("fallback", |x| {
            format!(
                "<mc:AlternateContent><mc:Choice Requires=\"zz\" xmlns:zz=\"urn:no\"><w:p><w:r><w:t>choice</w:t></w:r></w:p></mc:Choice><mc:Fallback>{x}</mc:Fallback></mc:AlternateContent>"
            )
        }),
    ]
}

const TABLE: &str = "<w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>";

/// The places a wrapper can stand where blocks stand, each as a function from the wrapper to the
/// body it makes. The identity wrapper gives the plain document that the others must read the same.
fn positions() -> Vec<Position> {
    vec![
        |w| format!("{}<w:p/>", w("<w:p/>")),
        |w| format!("<w:p/>{}", w(TABLE)),
        |w| {
            format!(
                "<w:tbl>{}<w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
                w("<w:tr><w:tc><w:p/></w:tc></w:tr>")
            )
        },
        |w| {
            format!(
                "<w:tbl><w:tr>{}<w:tc><w:p/></w:tc></w:tr></w:tbl>",
                w("<w:tc><w:p/></w:tc>")
            )
        },
        |w| {
            format!(
                "<w:tbl><w:tr><w:tc>{}<w:p/></w:tc></w:tr></w:tbl>",
                w("<w:p/>")
            )
        },
        |w| format!("<w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>", w(TABLE)),
        |w| w(&w("<w:p/>")),
        |w| w(&format!("<w:p/>{TABLE}<w:p/>")),
        |w| {
            format!(
                "<w:tbl><w:tr><w:tc><w:p/></w:tc>{}</w:tr></w:tbl><w:p/>",
                w(&w("<w:tc><w:p/><w:p/></w:tc>"))
            )
        },
    ]
}

#[test]
fn every_transparent_wrapper_is_walked_through_in_every_position() {
    for (n, position) in positions().into_iter().enumerate() {
        let plain = story(&position(&|x| x.to_string()));
        assert!(!plain.blocks().is_empty());
        for (name, wrap) in wrappers() {
            let s = story(&position(&|x| wrap(x)));
            assert_eq!(shape(&s), shape(&plain), "{name} in position {n}");
            assert!(s.opaque().is_empty(), "{name} in position {n}");
            assert!(s.anomalies().is_empty(), "{name} in position {n}");
            let controlled = s.blocks().iter().any(|e| e.has(IN_CONTROL));
            assert_eq!(controlled, name == "sdt", "{name} in position {n}");
            // the scan adds nothing of its own to a block's flags except the control
            assert!(s.blocks().iter().all(|e| e.flags & !IN_CONTROL == 0));
        }
    }
}

#[test]
fn a_wrapper_in_a_note_a_comment_or_a_header_is_walked_through_too() {
    for (name, wrap) in wrappers() {
        let inner = wrap("<w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>");
        let notes = part(
            "word/footnotes.xml",
            "footnotes",
            &format!(
                "<w:footnote w:id=\"2\">{inner}</w:footnote><w:footnote w:id=\"3\"><w:p/></w:footnote>"
            ),
        );
        let s = Story::scan(
            &Arc::from("word/footnotes.xml"),
            StoryKind::Footnotes,
            &notes,
        )
        .unwrap();
        assert_eq!(
            shape(&s),
            [(P, None), (T, None), (R, Some(1)), (P, Some(2)), (P, None)],
            "{name}"
        );
        assert_eq!(
            s.blocks().iter().map(|e| e.unit).collect::<Vec<_>>(),
            [0, 0, 0, 0, 1],
            "{name}"
        );
        let comments = part(
            "word/comments.xml",
            "comments",
            &format!("<w:comment w:id=\"1\" w:author=\"a\">{inner}</w:comment>"),
        );
        let c = Story::scan(
            &Arc::from("word/comments.xml"),
            StoryKind::Comments,
            &comments,
        )
        .unwrap();
        assert_eq!(c.blocks().len(), 4, "{name}");
        let header = part("word/header1.xml", "hdr", &inner);
        let h = Story::scan(&Arc::from("word/header1.xml"), StoryKind::Header, &header).unwrap();
        assert_eq!(h.blocks().len(), 4, "{name}");
        assert!(s.anomalies().is_empty() && c.anomalies().is_empty() && h.anomalies().is_empty());
    }
}

#[test]
fn a_wrapper_that_holds_no_block_contributes_nothing() {
    for (name, wrap) in wrappers() {
        let empty = wrap("");
        let marks = wrap(
            "<w:bookmarkStart w:id=\"1\" w:name=\"a\"/><w:proofErr/><w:bookmarkEnd w:id=\"1\"/>",
        );
        let s = story(&format!(
            "{empty}<w:p/>{marks}<w:tbl><w:tr>{}</w:tr></w:tbl>",
            wrap("")
        ));
        assert_eq!(shape(&s), [(P, None), (T, None), (R, Some(1))], "{name}");
        assert!(s.opaque().is_empty(), "{name}");
    }
    // a control with no content, and a branch with nothing taken
    let s = story("<w:sdt><w:sdtPr/></w:sdt><w:p/><mc:AlternateContent/>");
    assert_eq!(shape(&s), [(P, None)]);
}

#[test]
fn a_paragraph_in_a_body_level_move_or_a_deletion_is_a_block() {
    let s = story(
        "<w:moveFrom w:id=\"1\"><w:p><w:r><w:t>a</w:t></w:r></w:p></w:moveFrom><w:moveTo w:id=\"2\"><w:p/></w:moveTo>",
    );
    assert_eq!(shape(&s), [(P, None), (P, None)]);
}

// ---- opaque ---------------------------------------------------------------------------------------

#[test]
fn opaque_elements_are_listed_in_every_place_with_the_entry_before_them() {
    let tree = document(
        "<w:altChunk r:id=\"a\"/><w:p/><w14:foo xmlns:w14=\"urn:other\"/>\
         <w:tbl><w:tr><w:tc><w:p/><w:altChunk r:id=\"b\"/></w:tc><w:tc><m:oMath/><m:oMathPara/></w:tc></w:tr></w:tbl>\
         <w:sdt><w:sdtContent><w:altChunk r:id=\"c\"/></w:sdtContent></w:sdt><w:r><w:t>stray</w:t></w:r><w:t/><w:tab/>\
         <mc:AlternateContent><mc:Choice Requires=\"wps\"><w:foo/></mc:Choice></mc:AlternateContent><w:p/>",
    );
    let s = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    assert_eq!(
        shape(&s),
        [(P, None), (T, None), (R, Some(1)), (P, Some(2)), (P, None)]
    );
    let got: Vec<(OpaqueKind, Option<usize>)> =
        s.opaque().iter().map(|o| (o.kind, o.after)).collect();
    use OpaqueKind::*;
    assert_eq!(
        got,
        [
            (AltChunk, None),
            (Unknown, Some(0)),
            (AltChunk, Some(3)),
            (Math, Some(3)),
            (Math, Some(3)),
            (AltChunk, Some(3)),
            (Unknown, Some(3)),
            (Unknown, Some(3)),
            (Unknown, Some(3)),
            (Unknown, Some(3)),
        ]
    );
    // each names its element
    for o in s.opaque() {
        assert!(tree.node_ref(o.node).is_element());
    }
    assert_eq!(
        s.blocks().len(),
        5,
        "opaque elements are not blocks, so they have no ref"
    );
    assert!(s.anomalies().is_empty());
}

#[test]
fn opaque_elements_change_no_block_and_no_fingerprint() {
    let plain = "<w:p><w:r><w:t>a</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>";
    let noisy = "<w:altChunk r:id=\"a\"/><w:p><w:r><w:t>a</w:t></w:r></w:p><m:oMath/><w:tbl><w:tr><w:tc><w:p/><w:altChunk r:id=\"b\"/></w:tc></w:tr></w:tbl><w:foo/>";
    let (a, b) = (document(plain), document(noisy));
    let name: Arc<str> = Arc::from("word/document.xml");
    let (sa, sb) = (
        Story::scan(&name, StoryKind::Body, &a).unwrap(),
        Story::scan(&name, StoryKind::Body, &b).unwrap(),
    );
    assert_eq!(shape(&sa), shape(&sb));
    assert_eq!(sa.fingerprints(&a), sb.fingerprints(&b));
    assert!(sa.opaque().is_empty() && sb.opaque().len() == 4);
}

// ---- notes and comments ---------------------------------------------------------------------------

#[test]
fn notes_skip_their_separators_and_keep_everything_else_in_order() {
    for (root, unit, kind) in [
        ("footnotes", "footnote", StoryKind::Footnotes),
        ("endnotes", "endnote", StoryKind::Endnotes),
    ] {
        let tree = part(
            &format!("word/{root}.xml"),
            root,
            &format!(
                "<w:{unit} w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:{unit}>\
                 <w:{unit} w:type=\"continuationSeparator\" w:id=\"0\"><w:p/></w:{unit}>\
                 <w:{unit} w:type=\"continuationNotice\" w:id=\"1\"><w:p/></w:{unit}>\
                 <w:{unit} w:id=\"2\"><w:p/><w:p/></w:{unit}>\
                 <w:{unit} w:type=\"normal\" w:id=\"3\"><w:p/></w:{unit}>\
                 <w:p/><w:bookmarkStart w:id=\"9\" w:name=\"x\"/>\
                 <w:{unit} w:id=\"-7\"><w:p/></w:{unit}>\
                 <w:{unit} w:id=\"x\"><w:p/></w:{unit}>"
            ),
        );
        let s = Story::scan(&Arc::from(format!("word/{root}.xml")), kind, &tree).unwrap();
        assert_eq!(
            s.units().iter().map(|u| u.id).collect::<Vec<_>>(),
            [Some(2), Some(3), Some(-7), None],
            "{root}"
        );
        assert_eq!(
            s.blocks().iter().map(|e| e.unit).collect::<Vec<_>>(),
            [0, 0, 1, 2, 3],
            "{root}"
        );
        assert!(
            s.opaque().is_empty(),
            "stray content between notes is not a block"
        );
        assert_eq!(s.anomalies().len(), 1);
        assert_eq!(s.anomalies()[0].kind, AnomalyKind::UnitWithoutId);
    }
}

#[test]
fn a_comment_is_a_unit_whatever_its_type_says() {
    let tree = part(
        "word/comments.xml",
        "comments",
        "<w:comment w:id=\"0\" w:type=\"separator\" w:author=\"a\"><w:p/></w:comment>\
         <w:comment w:id=\"1\" w:author=\"b\" w:initials=\"B\" w:date=\"2025-01-01T00:00:00Z\"><w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:comment>",
    );
    let s = Story::scan(&Arc::from("word/comments.xml"), StoryKind::Comments, &tree).unwrap();
    assert_eq!(s.units().len(), 2);
    assert!(s.units().iter().all(|u| u.kind == UnitKind::Comment));
    assert_eq!(
        s.blocks().iter().map(|e| e.unit).collect::<Vec<_>>(),
        [0, 1, 1, 1, 1]
    );
    assert_eq!(
        s.unit_of(&s.blocks()[4])
            .and_then(|u| u.initials.as_deref()),
        Some("B")
    );
}

// ---- parts with the wrong root --------------------------------------------------------------------

#[test]
fn a_part_with_the_wrong_root_is_an_empty_story_with_one_anomaly_at_its_root() {
    let kinds = [
        (StoryKind::Header, "hdr"),
        (StoryKind::Footer, "ftr"),
        (StoryKind::Footnotes, "footnotes"),
        (StoryKind::Endnotes, "endnotes"),
        (StoryKind::Comments, "comments"),
    ];
    for (kind, _) in kinds {
        for (_, wrong) in kinds.iter().filter(|(k, _)| *k != kind) {
            let tree = part(
                "word/x.xml",
                wrong,
                "<w:p/><w:footnote w:id=\"1\"><w:p/></w:footnote>",
            );
            let s = Story::scan(&Arc::from("word/x.xml"), kind, &tree).unwrap();
            assert!(
                s.blocks().is_empty() && s.units().is_empty() && s.opaque().is_empty(),
                "{kind:?} read {wrong}"
            );
            assert_eq!(s.anomalies().len(), 1);
            assert_eq!(
                (s.anomalies()[0].kind, s.anomalies()[0].node),
                (AnomalyKind::WrongRoot, tree.root().id())
            );
            assert!(s.fingerprints(&tree).is_empty() && s.fields(&tree).is_empty());
        }
    }
    // the body is the one that can fail, and says which part and which root
    for (root, text) in [("hdr", "`w:hdr`"), ("footnotes", "`w:footnotes`")] {
        let tree = part("word/document.xml", root, "<w:body><w:p/></w:body>");
        let e = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap_err();
        assert_eq!(e.code(), ErrorCode::MalformedInput);
        assert!(
            e.message().starts_with("word/document.xml:") && e.message().contains(text),
            "{}",
            e.message()
        );
    }
    // another namespace under the right local name is not the right root
    let tree = crate::wordml::testing::parse(
        "word/document.xml",
        "<w:document xmlns:w=\"urn:other\"><w:body/></w:document>",
    );
    assert!(Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).is_err());
}
