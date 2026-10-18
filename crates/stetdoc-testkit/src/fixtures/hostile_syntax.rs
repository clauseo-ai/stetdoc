//! More hostile fixtures about the XML of `word/document.xml`: markup that is not well formed,
//! encodings the engine does not take, and nesting past the limit.

use super::parts::{DECL, Doc, entries_doc};
use super::zip::Zip;
use super::{DOCUMENT_XML, Expect, Fixture, hostile_fixture as fx};

const MALFORMED: &str = "malformed_input";
const LIMIT: &str = "limit_exceeded";

fn pkg(document: &[u8]) -> Vec<u8> {
    Zip::new(entries_doc(document)).build()
}

fn body(markup: &str) -> Vec<u8> {
    pkg(&Doc::new().body(markup).bytes())
}

fn run(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

fn declared(encoding: &str) -> Doc {
    Doc::new().decl(&format!(
        "<?xml version=\"1.0\" encoding=\"{encoding}\"?>\n"
    ))
}

/// Replaces the byte that stands for `@` with `with`.
fn swap_marker(mut bytes: Vec<u8>, with: u8) -> Vec<u8> {
    if let Some(at) = bytes.iter().position(|b| *b == b'@') {
        bytes[at] = with;
    }
    bytes
}

/// The UTF-16LE bytes of a document with an unpaired high surrogate where the `@` was.
fn utf16_with_lone_surrogate() -> Vec<u8> {
    let decl = "<?xml version=\"1.0\" encoding=\"UTF-16\" standalone=\"yes\"?>\n";
    let xml = Doc::new().decl(decl).body(&run("a@b")).xml();
    let mut out = vec![0xFF, 0xFE];
    for unit in xml.encode_utf16() {
        let unit = if unit == u16::from(b'@') {
            0xD800
        } else {
            unit
        };
        out.extend(unit.to_le_bytes());
    }
    out
}

/// The XML-level hostile fixtures that the first set did not cover.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "xml-doctype-internal-entity",
            "a DOCTYPE that declares one internal entity, which the text then uses",
            Expect::xml(MALFORMED),
            "xml.doctype.entity_declaration",
            || {
                let decl = format!("{DECL}<!DOCTYPE w:document [<!ENTITY e \"hello\">]>\n");
                pkg(&Doc::new().decl(&decl).body(&run("&e;")).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-doctype-bare",
            "a DOCTYPE with a name and nothing else",
            Expect::xml(MALFORMED),
            "xml.doctype",
            || {
                let decl = format!("{DECL}<!DOCTYPE w:document>\n");
                pkg(&Doc::new().decl(&decl).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-depth-300",
            "elements nested 300 levels deep, past the depth limit of 256",
            Expect::xml(LIMIT),
            "xml.depth_ge_100",
            || {
                let nested = format!("{}<w:p/>{}", "<w:x>".repeat(297), "</w:x>".repeat(297));
                body(&nested)
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-unclosed-element-at-eof",
            "the document ends after the paragraphs, before any end tag of the body or the root",
            Expect::xml(MALFORMED),
            "xml.unbalanced_tags_by_count",
            || {
                let xml = Doc::new().xml();
                let end = xml.find("<w:sectPr/>").unwrap_or(xml.len());
                pkg(&xml.as_bytes()[..end])
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-missing-root",
            "an XML declaration and whitespace, and no element",
            Expect::xml(MALFORMED),
            "xml.no_root_element_found",
            || pkg(format!("{DECL}\r\n").as_bytes()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-text-after-root",
            "non-whitespace text after the root element",
            Expect::xml(MALFORMED),
            "xml.epilog.junk",
            || pkg(&Doc::new().tail("trailing").bytes()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-lt-in-attribute-value",
            "a literal < inside an attribute value",
            Expect::xml(MALFORMED),
            "xml.attr.lt_in_value",
            || body("<w:p w:a=\"x<y\"/>"),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-unterminated-comment",
            "a comment that is opened before the root element and never closed",
            Expect::xml(MALFORMED),
            "xml.comment.unterminated",
            || {
                let decl = format!("{DECL}<!-- never closed ");
                pkg(&Doc::new().decl(&decl).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-unterminated-attribute-value",
            "an attribute value with no closing quote",
            Expect::xml(MALFORMED),
            "xml.attr.unterminated",
            || body("<w:p w:a=\"oops/></w:p>"),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-encoding-windows-1252",
            "the declaration names windows-1252, although every byte is ASCII, and OPC allows only UTF-8 and UTF-16",
            Expect::xml(MALFORMED),
            "xml.decl.encoding_not_utf",
            || pkg(&declared("windows-1252").bytes()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-encoding-iso-8859-1",
            "the declaration names iso-8859-1, although every byte is ASCII",
            Expect::xml(MALFORMED),
            "xml.decl.encoding_not_utf",
            || pkg(&declared("iso-8859-1").bytes()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-us-ascii-with-non-ascii-byte",
            "the declaration names us-ascii and a byte above 0x7F (0xE9) appears",
            Expect::xml(MALFORMED),
            "xml.decl_encoding_conflicts_with_bytes",
            || {
                let doc = declared("us-ascii").body(&run("Caf@")).bytes();
                pkg(&swap_marker(doc, 0xE9))
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-utf16-lone-surrogate",
            "a UTF-16LE part with an unpaired high surrogate, U+D800",
            Expect::xml(MALFORMED),
            "xml.invalid_encoding_bytes",
            || pkg(&utf16_with_lone_surrogate()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-comment-double-hyphen",
            "a comment that contains --, which XML 1.0 forbids although it changes no structure: the span tree accepts it and keeps its bytes",
            Expect::Ok,
            "xml.comment.double_hyphen",
            || body(&format!("<!-- a -- b -->{}", run("x"))),
        ),
    ]
}
