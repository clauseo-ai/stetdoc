//! Tests for what a part may hold: no character XML 1.0 forbids, and no XML version but 1.0.

use super::*;
use crate::api::error::ErrorCode;

fn parse(part: &str, bytes: &[u8]) -> Result<SpanTree, StetError> {
    SpanTree::parse(part, Bytes::from(bytes.to_vec()), &Limits::default())
}

#[test]
fn a_forbidden_character_anywhere_in_a_part_is_refused_with_its_offset() {
    // The same character in every place content can be, and in the XML declaration's neighbours.
    let places: [(&str, &str); 7] = [
        ("text", "<r>a{c}b</r>"),
        ("attribute value", "<r a=\"x{c}y\"/>"),
        ("comment", "<r><!-- a{c}b --></r>"),
        ("CDATA", "<r><![CDATA[a{c}b]]></r>"),
        ("processing instruction", "<r><?pi a{c}b?></r>"),
        ("prolog", "<!--a{c}b--><r/>"),
        ("epilog", "<r/><!--a{c}b-->"),
    ];
    let forbidden = [
        '\u{0}', '\u{1}', '\u{8}', '\u{b}', '\u{c}', '\u{e}', '\u{1f}', '\u{FFFE}', '\u{FFFF}',
    ];
    for (place, template) in places {
        for c in forbidden {
            let xml = template.replace("{c}", &c.to_string());
            let at = xml.find(c).unwrap();
            let e = parse("word/document.xml", xml.as_bytes())
                .err()
                .unwrap_or_else(|| panic!("U+{:04X} in {place} was accepted", u32::from(c)));
            assert_eq!(e.code(), ErrorCode::MalformedInput, "{place}");
            let want = format!(
                "word/document.xml: malformed XML at byte {at}: U+{:04X} is not a character XML 1.0 allows",
                u32::from(c)
            );
            assert_eq!(e.message(), want, "{place}");
        }
        // What is allowed, in the same places.
        for c in [
            '\t',
            '\n',
            '\r',
            '\u{7f}',
            '\u{85}',
            '\u{FFFD}',
            '\u{10000}',
            '\u{10FFFF}',
        ] {
            let xml = template.replace("{c}", &c.to_string());
            assert!(
                parse("p.xml", xml.as_bytes()).is_ok(),
                "U+{:04X} in {place}",
                u32::from(c)
            );
        }
    }
}

#[test]
fn a_forbidden_character_in_a_utf16_part_is_refused_and_the_offset_says_which_form() {
    // A part without a byte order mark is UTF-16 only if it starts with `<?`.
    let xml = "<?xml version=\"1.0\"?><r>a\u{1}b</r>";
    let at = xml.find('\u{1}').unwrap();
    for (big, bom) in [(false, true), (true, true), (false, false), (true, false)] {
        let bytes = encoding::utf8_to_utf16(xml.as_bytes(), big, bom);
        let e = parse("word/footer1.xml", &bytes).err().unwrap();
        assert_eq!(
            e.message(),
            format!(
                "word/footer1.xml: malformed XML at byte {at} of the UTF-8 form of this UTF-16 part: U+0001 is not a character XML 1.0 allows"
            ),
            "big endian {big}, mark {bom}"
        );
    }
}

#[test]
fn a_part_after_a_byte_order_mark_counts_offsets_from_the_mark() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(b"<r>\x01</r>");
    let e = parse("p.xml", &bytes).err().unwrap();
    assert!(e.message().contains("at byte 6:"), "{e}");
}

#[test]
fn only_xml_1_0_is_allowed_in_the_declaration() {
    for ok in [
        "<?xml version=\"1.0\"?><r/>",
        "<?xml version='1.0' encoding='UTF-8'?><r/>",
        "<?xml  version = \"1.0\"  standalone=\"yes\"?><r/>",
        "<r/>",
    ] {
        assert!(parse("p.xml", ok.as_bytes()).is_ok(), "{ok}");
    }
    for (bad, what) in [
        ("<?xml version=\"1.1\"?><r/>", "\"1.1\""),
        ("<?xml version='2.0' encoding='UTF-8'?><r/>", "\"2.0\""),
        ("<?xml version=\"1.0.1\"?><r/>", "\"1.0.1\""),
        ("<?xml version=\"\"?><r/>", "\"\""),
    ] {
        let e = parse("p.xml", bad.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{bad}"));
        assert_eq!(e.code(), ErrorCode::MalformedInput, "{bad}");
        assert!(
            e.message().contains(what) && e.message().contains("XML 1.0"),
            "{e}"
        );
    }
    let e = parse("p.xml", b"<?xml encoding=\"UTF-8\"?><r/>")
        .err()
        .unwrap();
    assert!(e.message().contains("no version"), "{e}");
    // The declaration of the other encodings is held to the same rule.
    let wide = encoding::utf8_to_utf16(
        b"<?xml version=\"1.1\" encoding=\"UTF-16\"?><r/>",
        false,
        true,
    );
    assert!(parse("p.xml", &wide).is_err());
}

#[test]
fn what_the_editor_refuses_to_write_the_parser_refuses_to_read() {
    // The two sides agree on which characters XML cannot carry, so a part the editor wrote always
    // parses again.
    for n in 0u32..0x80 {
        let c = char::from_u32(n).unwrap();
        let mut t = parse("p.xml", b"<r>x</r>").unwrap();
        let txt = t.root().first_child().unwrap().id();
        let edit_refused = t.edit().set_text(txt, &c.to_string()).is_err();
        let parse_refused = parse("p.xml", format!("<r>{c}</r>").as_bytes()).is_err();
        // `<` and `&` are markup in the second form, but the editor escapes them.
        if !matches!(c, '<' | '&') {
            assert_eq!(edit_refused, parse_refused, "U+{n:04X}");
        }
    }
    for c in ['\u{FFFE}', '\u{FFFF}'] {
        let mut t = parse("p.xml", b"<r>x</r>").unwrap();
        let txt = t.root().first_child().unwrap().id();
        assert!(t.edit().set_text(txt, &c.to_string()).is_err());
        assert!(parse("p.xml", format!("<r>{c}</r>").as_bytes()).is_err());
    }
}

#[test]
fn the_end_of_a_cdata_section_in_text_is_refused_and_is_fine_everywhere_else() {
    for xml in [
        "<r>a]]>b</r>",
        "<r>]]></r>",
        "<r>x<c/>y]]>z</r>",
        "<r><![CDATA[a]]>]]></r>",
    ] {
        let at = xml.rfind("]]>").unwrap();
        let e = parse("word/document.xml", xml.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{xml} was accepted"));
        assert_eq!(e.code(), ErrorCode::MalformedInput, "{xml}");
        let want = format!(
            "word/document.xml: malformed XML at byte {at}: the text contains ]]>, which only ends a CDATA section: write the > as &gt;"
        );
        assert_eq!(e.message(), want, "{xml}");
    }
    // The same characters that are not `]]>` in text, and `]]>` where XML allows it.
    for xml in [
        "<r>a]]&gt;b</r>",
        "<r>]]</r>",
        "<r>]></r>",
        "<r>] ]></r>",
        "<r>]<c/>]></r>",
        "<r>></r>",
        "<r a=\"]]>\"/>",
        "<r><!-- ]]> --></r>",
        "<r><![CDATA[a]]></r>",
        "<r><![CDATA[x]]>]></r>",
        "<r><![CDATA[x]]>&gt;</r>",
        "<r><?pi ]]>?></r>",
    ] {
        assert!(parse("p.xml", xml.as_bytes()).is_ok(), "{xml}");
    }
}

#[test]
fn what_the_editor_writes_never_has_the_end_of_a_cdata_section_in_text() {
    // The parser refuses `]]>` in text, so the editor has to write it as `]]&gt;`, in new text and
    // where an edit makes two pieces of text meet.
    let mut t = parse("p.xml", b"<r>a<c/>b</r>").unwrap();
    let first = t.root().first_child().unwrap().id();
    t.edit().set_text(first, "x]]>y").unwrap();
    let written = t.serialize();
    assert_eq!(written, b"<r>x]]&gt;y<c/>b</r>");
    parse("p.xml", &written).unwrap();

    let mut t = parse("p.xml", b"<r>a]]<c/>>b</r>").unwrap();
    let c = t.root().first_child().unwrap().next_sibling().unwrap().id();
    t.edit().detach(c).unwrap();
    let written = t.serialize();
    assert!(!String::from_utf8_lossy(&written).contains("]]>"));
    let again = parse("p.xml", &written).unwrap();
    assert_eq!(again.root().text_content(), "a]]>b");
}
