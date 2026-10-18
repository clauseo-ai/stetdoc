//! Tests of the markup fixtures: each `word/document.xml` carries the construct its description
//! names, and the edge ones are well formed.

use super::parts::Doc;
use super::tests::{find, part_bytes};

fn document(name: &str) -> Vec<u8> {
    part_bytes(&find(name).bytes(), "word/document.xml")
}

fn text(name: &str) -> String {
    String::from_utf8(document(name)).unwrap()
}

/// The deepest element nesting of a document whose markup is plain: no `>` inside a value.
fn max_depth(xml: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    let mut rest = xml;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        if rest.starts_with("<!--") {
            rest = &rest[rest.find("-->").map_or(rest.len(), |i| i + 3)..];
            continue;
        }
        let end = rest.find('>').map_or(rest.len(), |i| i + 1);
        let tag = &rest[..end];
        if tag.starts_with("</") {
            depth -= 1;
        } else if !tag.starts_with("<?") && !tag.starts_with("<!") {
            deepest = deepest.max(depth + 1);
            if !tag.ends_with("/>") {
                depth += 1;
            }
        }
        rest = &rest[end..];
    }
    deepest
}

#[test]
fn the_declarations_name_the_encodings_they_are_named_for() {
    for (name, declared) in [
        ("xml-declaration-us-ascii", "encoding=\"us-ascii\""),
        ("xml-declaration-ascii", "encoding=\"ascii\""),
        ("xml-encoding-windows-1252", "encoding=\"windows-1252\""),
        ("xml-encoding-iso-8859-1", "encoding=\"iso-8859-1\""),
        ("xml-us-ascii-with-non-ascii-byte", "encoding=\"us-ascii\""),
    ] {
        assert!(
            text_lossy(name).starts_with(&format!("<?xml version=\"1.0\" {declared}?>")),
            "{name}"
        );
    }
    for name in [
        "xml-declaration-us-ascii",
        "xml-declaration-ascii",
        "xml-encoding-windows-1252",
    ] {
        assert!(document(name).is_ascii(), "{name}");
    }
    let stray = document("xml-us-ascii-with-non-ascii-byte");
    assert_eq!(
        stray.iter().filter(|b| **b > 127).collect::<Vec<_>>(),
        [&0xE9]
    );
}

fn text_lossy(name: &str) -> String {
    String::from_utf8_lossy(&document(name)).into_owned()
}

#[test]
fn the_prolog_and_epilog_hold_comments_and_instructions() {
    let xml = text("xml-prolog-and-epilog-comments-and-pis");
    let root = xml.find("<w:document").unwrap();
    let end = xml.find("</w:document>").unwrap();
    assert!(xml[..root].contains("<!-- prolog comment -->"));
    assert!(xml[..root].contains("<?prolog-pi some data?>"));
    assert!(xml[end..].contains("<!-- epilog comment -->"));
    assert!(xml[end..].contains("<?epilog-pi more data?>"));
}

#[test]
fn the_text_constructs_are_written_as_described() {
    assert!(text("xml-cdata-split-around-terminator").contains("<![CDATA[x]]]]><![CDATA[>y]]>"));
    assert!(text("xml-cdata-split-around-terminator").contains("<!-- a - b - c -->"));
    assert!(
        text("xml-space-preserve-padded-text")
            .contains("xml:space=\"preserve\">  lead  and  trail  </w:t>")
    );
    assert!(
        text("xml-mixed-content-in-paragraph")
            .contains("<w:p>before<w:r><w:t>in</w:t></w:r>between<w:r>")
    );
    assert!(text("xml-mixed-content-in-paragraph").contains("</w:r>after</w:p>"));
    assert!(text("opc-empty-body").contains("<w:body><w:sectPr/></w:body>"));
}

#[test]
fn the_attribute_and_namespace_edge_cases_are_in_the_bytes() {
    let xml = text("xml-gt-in-attribute-value");
    assert!(xml.contains("<w:lang w:val='x > y'/>"));
    assert!(xml.contains("w:rsidRPr = \"00B2\""));
    let xml = text("xml-ns-prefix-rebound-in-siblings");
    for uri in ["urn:one", "urn:two", "urn:three"] {
        assert_eq!(
            xml.matches(&format!("xmlns:q=\"{uri}\"")).count(),
            1,
            "{uri}"
        );
    }
    assert!(xml.find("urn:one").unwrap() < xml.find("urn:two").unwrap());
}

#[test]
fn the_long_values_have_the_lengths_claimed() {
    let xml = text("xml-text-node-200000-chars");
    let start = xml.find("<w:t>").unwrap() + 5;
    let end = xml.find("</w:t>").unwrap();
    assert_eq!(end - start, 200_000);
    let xml = text("xml-attribute-value-100000-chars");
    let start = xml.find("w:rsidR=\"").unwrap() + 9;
    let end = start + xml[start..].find('"').unwrap();
    assert_eq!(end - start, 100_000);
}

#[test]
fn the_nesting_depths_are_exact() {
    assert_eq!(max_depth(&text("xml-depth-100")), 100);
    assert_eq!(max_depth(&text("xml-depth-300")), 300);
    assert_eq!(
        max_depth(&Doc::new().xml()),
        5,
        "document, body, paragraph, run, text"
    );
}

#[test]
fn the_malformed_documents_carry_the_defect_in_the_name() {
    assert!(
        text("xml-doctype-internal-entity")
            .contains("<!DOCTYPE w:document [<!ENTITY e \"hello\">]>")
    );
    assert!(text("xml-doctype-internal-entity").contains("&e;"));
    assert!(text("xml-doctype-bare").contains("<!DOCTYPE w:document>\n<w:document"));
    let cut = text("xml-unclosed-element-at-eof");
    assert!(
        cut.contains("<w:body>") && !cut.contains("</w:body>") && !cut.contains("</w:document>")
    );
    let empty = text("xml-missing-root");
    assert!(!empty.contains("<w:") && empty.starts_with("<?xml"));
    assert!(text("xml-text-after-root").ends_with("</w:document>trailing"));
    assert!(text("xml-lt-in-attribute-value").contains("w:a=\"x<y\""));
    let comment = text("xml-unterminated-comment");
    assert!(!comment[comment.find("<!--").unwrap()..].contains("-->"));
    assert!(text("xml-unterminated-attribute-value").contains("w:a=\"oops/></w:p>"));
    assert!(text("xml-comment-double-hyphen").contains("<!-- a -- b -->"));
}

#[test]
fn the_utf16_part_has_an_unpaired_surrogate_and_nothing_else_wrong() {
    let bytes = document("xml-utf16-lone-surrogate");
    assert_eq!(&bytes[..2], [0xFF, 0xFE]);
    assert_eq!(bytes.len() % 2, 0);
    let units: Vec<u16> = bytes[2..]
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let broken = char::decode_utf16(units.iter().copied())
        .filter(Result::is_err)
        .count();
    assert_eq!(broken, 1);
    assert_eq!(
        units
            .iter()
            .filter(|u| (0xD800..0xE000).contains(*u))
            .count(),
        1
    );
}
