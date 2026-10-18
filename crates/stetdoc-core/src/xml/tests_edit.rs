//! Tests for what edits may write, what reads report, and what errors say.

use super::*;
use crate::api::error::ErrorCode;

fn parse(s: &str) -> SpanTree {
    SpanTree::parse(
        "test.xml",
        Bytes::from(s.as_bytes().to_vec()),
        &Limits::default(),
    )
    .expect("parse")
}

fn out(t: &SpanTree) -> String {
    String::from_utf8(t.serialize()).expect("utf-8")
}

#[test]
fn edits_refuse_characters_xml_cannot_carry_and_change_nothing() {
    let src = "<r a=\"1\"><t>text</t></r>";
    for bad in ["\u{1}", "x\u{0}y", "\u{b}", "\u{FFFE}", "\u{FFFF}"] {
        let mut t = parse(src);
        let root = t.root().id();
        let txt = t.root().first_child().unwrap().first_child().unwrap().id();
        let mut ed = t.edit();
        for e in [
            ed.set_text(txt, bad).unwrap_err(),
            ed.replace_text_range(txt, 0, 1, bad).unwrap_err(),
            ed.create_text(bad).unwrap_err(),
            ed.set_attr(root, "", "a", bad).unwrap_err(),
            ed.set_attr(root, "", "new", bad).unwrap_err(),
        ] {
            assert_eq!(e.code(), ErrorCode::InvalidPayload, "{bad:?}: {e}");
            assert!(e.message().contains("U+"), "{e}");
        }
        assert!(!t.is_dirty(), "a refused edit marks nothing: {bad:?}");
        assert_eq!(out(&t), src);
    }
    // Everything else, including characters outside the first plane and the C1 controls, is fine.
    let mut t = parse(src);
    let txt = t.root().first_child().unwrap().first_child().unwrap().id();
    t.edit()
        .set_text(txt, "a\tb\n\u{85}\u{10000}\u{FFFD}")
        .unwrap();
    assert_eq!(
        parse(&out(&t)).root().text_content(),
        "a\tb\n\u{85}\u{10000}\u{FFFD}"
    );
}

#[test]
fn created_names_must_be_valid_xml_names() {
    let mut t = parse("<r/>");
    let mut ed = t.edit();
    for bad in [
        "",
        "1a",
        "a b",
        "a:b",
        "a\u{1}",
        "a\u{FFFF}",
        "\u{a0}",
        "a\u{d7}",
    ] {
        let e = ed.create_element("urn:x", bad).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{bad:?}");
    }
    assert!(ed.create_element("urn:x", "caf\u{e9}").is_ok());
}

#[test]
fn removing_a_namespace_declaration_is_refused() {
    let src = "<r xmlns=\"urn:d\" xmlns:w=\"urn:w\" a=\"1\"><w:p/></r>";
    let mut t = parse(src);
    let root = t.root().id();
    let mut ed = t.edit();
    for (ns, local) in [
        ("http://www.w3.org/2000/xmlns/", "w"),
        ("http://www.w3.org/2000/xmlns/", "xmlns"),
        ("", "xmlns"),
    ] {
        let e = ed.remove_attr(root, ns, local).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{ns} {local}");
    }
    assert!(!t.is_dirty());
    assert!(t.edit().remove_attr(root, "", "a").unwrap());
    assert_eq!(out(&t), "<r xmlns=\"urn:d\" xmlns:w=\"urn:w\"><w:p/></r>");
    assert!(
        SpanTree::parse(
            "test.xml",
            Bytes::from(out(&t).into_bytes()),
            &Limits::default()
        )
        .is_ok()
    );
}

#[test]
fn text_and_attribute_values_read_as_an_xml_parser_reports_them() {
    let t = parse(
        "<r a=\"x\ty\r\nz\rw\nv&#9;&#10;&#13;\"><![CDATA[c\r\nd&amp;]]><!--m\rn-->t1\r\nt2\rt3&#13;t4</r>",
    );
    let r = t.root();
    assert_eq!(r.attr("", "a").unwrap(), "x y z w v\t\n\r");
    let kids: Vec<_> = r.children().collect();
    assert_eq!(kids[0].text(), "c\nd&amp;");
    assert_eq!(kids[1].text(), "m\nn");
    assert_eq!(kids[2].text(), "t1\nt2\nt3\rt4");
}

#[test]
fn replacing_a_range_counts_a_line_ending_as_one_character() {
    let mut t = parse("<r>ab\r\ncd\ref</r>");
    let txt = t.root().first_child().unwrap();
    assert_eq!(txt.text(), "ab\ncd\nef");
    let id = txt.id();
    // Characters 2 and 3 are the line feed and `c`: `\r\n` is one character, and so is `\r`.
    t.edit().replace_text_range(id, 2, 4, "-").unwrap();
    assert_eq!(out(&t), "<r>ab-d\ref</r>");
    t.edit().replace_text_range(id, 4, 5, "+").unwrap();
    assert_eq!(out(&t), "<r>ab-d+ef</r>");
}

#[test]
fn an_error_names_the_part_and_says_when_offsets_are_in_the_utf8_form() {
    let bad = "<r>\n<a></b></r>";
    let e = SpanTree::parse(
        "word/document.xml",
        Bytes::from(bad.as_bytes().to_vec()),
        &Limits::default(),
    )
    .err()
    .unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(
        e.message()
            .starts_with("word/document.xml: malformed XML at byte 7:"),
        "{e}"
    );
    assert!(!e.message().contains("UTF-16"), "{e}");
    // The same fault in a UTF-16 part: the offset is in the UTF-8 form, and the message says so.
    let wide = encoding::utf8_to_utf16(bad.as_bytes(), false, true);
    let e = SpanTree::parse("word/footer1.xml", Bytes::from(wide), &Limits::default())
        .err()
        .unwrap();
    assert!(
        e.message().starts_with(
            "word/footer1.xml: malformed XML at byte 7 of the UTF-8 form of this UTF-16 part:"
        ),
        "{e}"
    );
    // Limit errors and encoding errors name the part too.
    let deep = format!("{}{}", "<a>".repeat(300), "</a>".repeat(300));
    let e = SpanTree::parse(
        "word/deep.xml",
        Bytes::from(deep.into_bytes()),
        &Limits::default(),
    )
    .err()
    .unwrap();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(e.message().starts_with("word/deep.xml: "), "{e}");
    let e = SpanTree::parse(
        "word/x.xml",
        Bytes::from(vec![0xFF, 0xFE, 0x41]),
        &Limits::default(),
    )
    .err()
    .unwrap();
    assert!(e.message().starts_with("word/x.xml: "), "{e}");
}

#[test]
fn a_raised_attribute_limit_cannot_make_the_count_wrap() {
    // The attributes of an element are stored in one chunk of 4,096, and are counted in 16 bits, so
    // no limit that a caller raises lets an element have more.
    let lim = Limits {
        max_xml_attributes: u32::MAX,
        ..Limits::default()
    };
    for count in [70_000, 5_000] {
        let attrs: String = (0..count).map(|i| format!(" a{i}=\"\"")).collect();
        let src = format!("<r{attrs}/>");
        let e = SpanTree::parse("test.xml", Bytes::from(src.into_bytes()), &lim)
            .err()
            .expect("more attributes than the tree can hold");
        assert_eq!(e.code(), ErrorCode::LimitExceeded);
        assert!(e.message().contains("more than 4096 attributes"), "{e}");
    }
    // Exactly a chunk of them is fine, and is stored whole.
    let attrs: String = (0..4096).map(|i| format!(" a{i}=\"\"")).collect();
    let src = format!("<r{attrs}><c d=\"1\"/></r>");
    let t = SpanTree::parse("test.xml", Bytes::from(src.clone().into_bytes()), &lim).unwrap();
    assert_eq!(t.root().attrs().count(), 4096);
    assert_eq!(t.root().first_child().unwrap().attr("", "d").unwrap(), "1");
    assert_eq!(t.serialize(), src.as_bytes());
}

#[test]
fn a_prefix_too_long_to_count_is_refused() {
    let prefix = "p".repeat(70_000);
    let src = format!("<{prefix}:r xmlns:{prefix}=\"urn:x\"/>");
    let e = SpanTree::parse(
        "test.xml",
        Bytes::from(src.into_bytes()),
        &Limits::default(),
    )
    .err()
    .expect("a 70,000 byte prefix");
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
}

#[test]
fn offsets_beyond_four_gibibytes_are_an_error_and_not_a_clamp() {
    assert!(tree::Span::new(1, 2).is_ok());
    let big = u32::MAX as usize + 1;
    let e = tree::Span::new(0, big).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(tree::Span::new(big, big + 1).is_err());
}

#[test]
fn the_editor_keeps_to_the_limits_the_part_was_parsed_with() {
    let lim = Limits {
        max_xml_nodes: 12,
        max_xml_attributes: 3,
        ..Limits::default()
    };
    let mut t = SpanTree::parse(
        "test.xml",
        Bytes::from(b"<r a=\"1\" b=\"2\"/>".to_vec()),
        &lim,
    )
    .unwrap();
    let root = t.root().id();
    let mut ed = t.edit();
    ed.set_attr(root, "", "c", "3").unwrap();
    let e = ed.set_attr(root, "", "d", "4").unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
    // Setting an attribute again and again does not grow the arena: the old list is reused.
    for i in 0..100 {
        ed.set_attr(root, "", "c", &i.to_string()).unwrap();
    }
    assert!(t.attrs.len() <= 8, "{}", t.attrs.len());
    let mut ed = t.edit();
    let mut made = 0;
    loop {
        match ed.create_text("x") {
            Ok(_) => made += 1,
            Err(e) => {
                assert_eq!(e.code(), ErrorCode::LimitExceeded);
                break;
            }
        }
        assert!(made < 100, "the node limit is never reached");
    }
    assert!(made >= 5);
}
