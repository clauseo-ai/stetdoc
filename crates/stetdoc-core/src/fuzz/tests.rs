//! Tests of the fuzz entry points: they accept what is fine, and the checks they make can fail.

use super::{package_laws, xml_edits, xml_laws};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::xml::{Bytes, SpanTree};

const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP of stored parts, written by hand so the tests need no other writer. With `zip64`, the
/// end records are the ZIP64 ones, and the locator points `shift` bytes away from the record.
fn zip_with(parts: &[(&str, &[u8])], zip64: Option<i64>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in parts {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let size = data.len() as u32;
        let name = name.as_bytes();
        out.extend(b"PK\x03\x04");
        out.extend([20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend((name.len() as u16).to_le_bytes());
        out.extend([0, 0]);
        out.extend(name);
        out.extend(*data);
        central.extend(b"PK\x01\x02");
        central.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend((name.len() as u16).to_le_bytes());
        central.extend([0u8; 12]);
        central.extend(offset.to_le_bytes());
        central.extend(name);
    }
    let start = out.len() as u32;
    out.extend(&central);
    let count = parts.len() as u16;
    if let Some(shift) = zip64 {
        let record = out.len() as i64;
        out.extend(b"PK\x06\x06");
        out.extend(44u64.to_le_bytes());
        out.extend([45, 0, 45, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        out.extend(u64::from(count).to_le_bytes());
        out.extend(u64::from(count).to_le_bytes());
        out.extend((central.len() as u64).to_le_bytes());
        out.extend(u64::from(start).to_le_bytes());
        out.extend(b"PK\x06\x07\0\0\0\0");
        out.extend(((record + shift) as u64).to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(b"PK\x05\x06\0\0\0\0");
        out.extend([0xFF; 4]);
        out.extend([0xFF; 8]);
        out.extend([0, 0]);
    } else {
        out.extend(b"PK\x05\x06\0\0\0\0");
        out.extend(count.to_le_bytes());
        out.extend(count.to_le_bytes());
        out.extend((central.len() as u32).to_le_bytes());
        out.extend(start.to_le_bytes());
        out.extend([0, 0]);
    }
    out
}

fn zip(parts: &[(&str, &[u8])]) -> Vec<u8> {
    zip_with(parts, None)
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
const RELS: &str = "<?xml version=\"1.0\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";
const DOC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p w:rsidR=\"00A1\"><w:r><w:t>Hello</w:t></w:r></w:p></w:body></w:document>\n";

/// Every shape of markup the tree reads: a prolog and an epilog, a declaration of a namespace
/// below the root, single quotes, references, CDATA, comments and a processing instruction.
const RICH: &str = "<?xml version='1.0' encoding='UTF-8'?>\r\n<!-- prolog --><?pi data?>\r\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"w14\"><w:body><w:p w:a='1' w:b = \"2 &gt; 1\"><w:r xmlns:x=\"urn:x\" x:flag=\"1\"><w:t xml:space=\"preserve\"> a &amp; b &#xA0;&#65; <![CDATA[<c>]]> tail </w:t><!-- inside --></w:r></w:p><w:p/><w:p></w:p></w:body></w:document>\r\n<!-- epilog -->";

fn package_bytes() -> Vec<u8> {
    zip(&[
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("word/document.xml", DOC.as_bytes()),
    ])
}

fn with_head(head: [u8; 4], doc: &[u8]) -> Vec<u8> {
    let mut data = head.to_vec();
    data.extend(doc);
    data
}

fn utf16(text: &str, big_endian: bool) -> Vec<u8> {
    let mut out = if big_endian {
        vec![0xFE, 0xFF]
    } else {
        vec![0xFF, 0xFE]
    };
    for unit in text.encode_utf16() {
        out.extend(if big_endian {
            unit.to_be_bytes()
        } else {
            unit.to_le_bytes()
        });
    }
    out
}

#[test]
fn the_hand_written_package_opens_and_has_three_parts() {
    let pkg = crate::package::Package::open(package_bytes(), &Limits::default()).expect("opens");
    assert_eq!(pkg.part_names().count(), 3);
}

#[test]
fn an_input_too_short_for_the_choices_does_nothing() {
    for data in [&[][..], &[1], &[1, 2, 3]] {
        super::package(data);
        super::xml(data);
    }
}

#[test]
fn a_package_passes_every_choice_of_part_and_replacement() {
    let doc = package_bytes();
    for pick in 0..=4u8 {
        for mode in 0..4u8 {
            for (hi, lo) in [(0, 0), (0, 7), (1, 1), (0, 255)] {
                let data = with_head([pick, mode, hi, lo], &doc);
                assert_eq!(
                    package_laws::check(&data, &Limits::default()),
                    Ok(()),
                    "{pick} {mode} {hi} {lo}"
                );
            }
        }
    }
}

fn three_parts() -> [(&'static str, &'static [u8]); 3] {
    [
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", RELS.as_bytes()),
        ("word/document.xml", DOC.as_bytes()),
    ]
}

#[test]
fn a_zip64_package_passes_whether_the_locator_is_right_or_open_refuses_it() {
    for shift in [0, 2, -2, 40, -1000] {
        let doc = zip_with(&three_parts(), Some(shift));
        for pick in 0..3u8 {
            let data = with_head([pick, 0, 0, 0], &doc);
            assert_eq!(
                package_laws::check(&data, &Limits::default()),
                Ok(()),
                "shift {shift}, part {pick}"
            );
        }
    }
}

#[test]
fn a_package_that_is_refused_is_not_a_failure() {
    let good = package_bytes();
    let cut = good.get(..good.len() - 30).expect("shorter").to_vec();
    for doc in [&[][..], b"not a zip at all", &good[..10], &cut] {
        let data = with_head([0, 0, 0, 0], doc);
        assert_eq!(package_laws::check(&data, &Limits::default()), Ok(()));
    }
    let tight = Limits {
        max_entries: 2,
        ..Limits::default()
    };
    assert_eq!(
        package_laws::check(&with_head([0; 4], &good), &tight),
        Ok(())
    );
}

#[test]
fn the_four_ways_to_make_a_replacement_differ() {
    let own = b"abcdef".to_vec();
    assert_eq!(package_laws::replacement(0, 3, 9, Some(&own)), own);
    let flipped = package_laws::replacement(1, 2, 9, Some(&own));
    assert_eq!(flipped.len(), 6);
    assert_eq!(flipped.iter().zip(&own).filter(|(a, b)| a != b).count(), 1);
    assert_eq!(package_laws::replacement(2, 3, 9, Some(&own)), b"abc");
    assert_eq!(
        package_laws::replacement(3, 2, 9, Some(&own)),
        vec![9u8; 32]
    );
    assert_eq!(package_laws::replacement(0, 2, 9, None), vec![9u8; 32]);
}

#[test]
fn a_part_passes_every_kind_of_edit_on_every_encoding() {
    let inputs = [
        DOC.as_bytes().to_vec(),
        RICH.as_bytes().to_vec(),
        [b"\xEF\xBB\xBF".as_slice(), RICH.as_bytes()].concat(),
        utf16(RICH, false),
        utf16(RICH, true),
    ];
    for (n, doc) in inputs.iter().enumerate() {
        for kind in 0..8u8 {
            for (hi, lo) in [(0, 0), (0, 1), (0, 2), (0, 3), (1, 0), (0, 200)] {
                for v in 0..10u8 {
                    let data = with_head([kind, hi, lo, v], doc);
                    assert_eq!(
                        xml_laws::check(&data, &Limits::default()),
                        Ok(()),
                        "input {n}, kind {kind}, at {hi}/{lo}, value {v}"
                    );
                }
            }
        }
    }
}

#[test]
fn hostile_parts_are_refused_and_that_is_not_a_failure() {
    let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
    let many_attrs = format!(
        "<a {}/>",
        (0..2000)
            .map(|i| format!("b{i}=\"\""))
            .collect::<Vec<_>>()
            .join(" ")
    );
    for doc in [
        "<!DOCTYPE a [<!ENTITY e \"x\">]><a>&e;</a>",
        "<a><b></a>",
        "<a>&undefined;</a>",
        "<?xml version=\"1.1\"?><a/>",
        "<a>\u{1}</a>",
        "",
        "   ",
        deep.as_str(),
        many_attrs.as_str(),
    ] {
        let data = with_head([4, 0, 0, 0], doc.as_bytes());
        assert_eq!(
            xml_laws::check(&data, &Limits::default()),
            Ok(()),
            "{doc:.40}"
        );
    }
    let bad_utf8 = with_head([0; 4], b"<a>\xC3\x28</a>");
    assert_eq!(xml_laws::check(&bad_utf8, &Limits::default()), Ok(()));
}

#[test]
fn a_refusal_must_have_the_right_code_and_name_the_part() {
    let named = StetError::new(ErrorCode::MalformedInput, "word/document.xml: broken");
    assert_eq!(xml_laws::refused(&named), Ok(()));
    let unnamed = StetError::new(ErrorCode::MalformedInput, "broken");
    assert!(xml_laws::refused(&unnamed).is_err());
    let wrong = StetError::new(ErrorCode::InvalidPayload, "word/document.xml: broken");
    assert!(xml_laws::refused(&wrong).is_err());
}

fn parse(text: &str) -> SpanTree {
    SpanTree::parse(
        "word/document.xml",
        Bytes::from(text.as_bytes()),
        &Limits::default(),
    )
    .expect("parses")
}

#[test]
fn a_clean_node_that_is_missing_from_the_output_is_caught() {
    let mut edited = parse(DOC);
    let p = edited
        .root()
        .child_elements()
        .next()
        .and_then(|b| b.child_elements().next())
        .expect("w:p")
        .id();
    edited
        .edit()
        .set_attr(p, NS_W, "rsidRPr", "00B2")
        .expect("sets");
    let out = String::from_utf8(edited.serialize()).expect("utf-8");
    assert!(out.contains("rsidRPr"), "{out}");
    assert_eq!(
        xml_laws::clean_nodes_are_in_the_output(&edited, &parse(&out)),
        Ok(())
    );
    let tampered = out.replace("Hello", "Hellp");
    let found = xml_laws::clean_nodes_are_in_the_output(&edited, &parse(&tampered));
    assert!(
        found.is_err_and(|e| e.contains("clean")),
        "a changed clean node"
    );
    let reordered = out.replace("<w:body>", "<w:body><w:p><w:r><w:t>Hello</w:t></w:r></w:p>");
    assert_eq!(
        xml_laws::clean_nodes_are_in_the_output(&edited, &parse(&reordered)),
        Ok(()),
        "an extra node does no harm, only a missing or reordered one does"
    );
}

#[test]
fn the_dump_ignores_prefixes_and_attribute_order_but_not_values() {
    let a = parse("<w:a xmlns:w=\"urn:w\" w:x=\"1\" w:y=\"2\">t</w:a>");
    let b = parse("<q:a xmlns:q=\"urn:w\" q:y=\"2\" q:x=\"1\">t</q:a>");
    let c = parse("<w:a xmlns:w=\"urn:w\" w:x=\"1\" w:y=\"3\">t</w:a>");
    assert_eq!(xml_laws::dump(&a), xml_laws::dump(&b));
    assert_ne!(xml_laws::dump(&a), xml_laws::dump(&c));
    let d = parse("<w:a xmlns:w=\"urn:other\" w:x=\"1\" w:y=\"2\">t</w:a>");
    assert_ne!(xml_laws::dump(&a), xml_laws::dump(&d));
}

#[test]
fn a_node_whose_handle_means_something_else_is_caught() {
    let tree = parse(DOC);
    let mut before = xml_laws::identities(&tree);
    assert!(before.len() > 5);
    assert_eq!(xml_laws::identities_are_kept(&tree, &before), Ok(()));
    before.get_mut(2).expect("a third node").2.push('x');
    let found = xml_laws::identities_are_kept(&tree, &before);
    assert!(found.is_err_and(|e| e.contains("handle")), "a renamed node");
    let mut other_kind = xml_laws::identities(&tree);
    other_kind.get_mut(0).expect("the document").1 = crate::xml::NodeKind::Text;
    assert!(xml_laws::identities_are_kept(&tree, &other_kind).is_err());
}

#[test]
fn a_carriage_return_written_outside_the_root_element_is_expected_to_read_as_a_line_feed() {
    for head in [[2u8, 0, 0, 4], [3, 0, 0, 24]] {
        let mut tree = parse(DOC);
        let edit = xml_edits::plan(&tree, head).expect("an edit");
        assert_eq!(xml_edits::apply(&mut tree, &edit), Ok(()), "{head:?}");
        let outside = tree
            .document()
            .children()
            .find(|n| n.kind() == crate::xml::NodeKind::Text)
            .expect("white space after the declaration");
        let text = outside.text();
        assert!(
            text.contains('\t'),
            "the edit was made, not refused: {head:?} {text:?}"
        );
        assert!(
            !text.contains('\r'),
            "a carriage return reads as a line feed: {text:?}"
        );
        let data = with_head(head, DOC.as_bytes());
        assert_eq!(
            xml_laws::check(&data, &Limits::default()),
            Ok(()),
            "{head:?}"
        );
    }
}

#[test]
fn text_that_meets_text_after_a_detach_is_dirty_and_written_joined_and_passes() {
    // Detaches the element at index 1 of what is below the root: the element between two texts.
    // The two texts are dirty after it, so the law asks nothing of how they are written, and the
    // output must still parse and agree with the tree.
    for doc in [
        "<a>x\r<b/>\ny</a>",
        "<a>x\r\r\r<b/>\ny</a>",
        "<a>x\r<!--c-->\ny</a>",
        "<a>x]]<b/>>y</a>",
        "<a>x]<b/>]>y</a>",
        "<a>x]]<b/>>\ny</a>",
        "<a>x&#13;<b/>\ny</a>",
        "<a>x<b/>\ny</a>",
    ] {
        let data = with_head([6, 0, 1, 0], doc.as_bytes());
        assert_eq!(
            xml_laws::check(&data, &Limits::default()),
            Ok(()),
            "{doc:?}"
        );
    }
}

#[test]
fn a_rewrite_of_a_clean_text_is_caught() {
    let edited = parse("<a>x\r<b/>y</a>");
    let rewritten = parse("<a>x\n<b/>y</a>");
    let found = xml_laws::clean_nodes_are_in_the_output(&edited, &rewritten);
    assert!(
        found.is_err_and(|e| e.contains("not in the part")),
        "a carriage return"
    );
    let edited = parse("<a>]]<b/>>y</a>");
    let rewritten = parse("<a>]]<b/>&gt;y</a>");
    let found = xml_laws::clean_nodes_are_in_the_output(&edited, &rewritten);
    assert!(found.is_err_and(|e| e.contains("not in the part")), "a >");
}
