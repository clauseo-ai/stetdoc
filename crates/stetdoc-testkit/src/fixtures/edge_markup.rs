//! More edge fixtures about the XML of `word/document.xml`: declarations, the prolog and the
//! epilog, CDATA, mixed content and very long values. All are well-formed and must open and
//! round-trip byte for byte.

use super::parts::{DECL, Doc, entries_doc};
use super::zip::Zip;
use super::{Expect, Fixture, edge_fixture as fx};

const OK: Expect = Expect::Ok;

fn pkg(document: &[u8]) -> Vec<u8> {
    Zip::new(entries_doc(document)).build()
}

fn declared(encoding: &str) -> Vec<u8> {
    Doc::new()
        .decl(&format!(
            "<?xml version=\"1.0\" encoding=\"{encoding}\"?>\n"
        ))
        .bytes()
}

fn long_text() -> Vec<u8> {
    let text: String = "Lorem ipsum dolor sit amet "
        .repeat(8000)
        .chars()
        .take(200_000)
        .collect();
    pkg(&Doc::new()
        .body(&format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"))
        .bytes())
}

fn long_attribute() -> Vec<u8> {
    let value = "A1b2".repeat(25_000);
    pkg(&Doc::new()
        .body(&format!(
            "<w:p w:rsidR=\"{value}\"><w:r><w:t>t</w:t></w:r></w:p>"
        ))
        .bytes())
}

/// The XML-level edge fixtures that the first set did not cover.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "xml-declaration-us-ascii",
            "the declaration names encoding=\"us-ascii\" and every byte is ASCII",
            OK,
            || pkg(&declared("us-ascii")),
        ),
        fx(
            "xml-declaration-ascii",
            "the declaration names encoding=\"ascii\" and every byte is ASCII",
            OK,
            || pkg(&declared("ascii")),
        ),
        fx(
            "xml-prolog-and-epilog-comments-and-pis",
            "a comment and a processing instruction in the prolog and again after the root element",
            OK,
            || {
                let decl = format!("{DECL}<!-- prolog comment -->\n<?prolog-pi some data?>\n");
                pkg(&Doc::new()
                    .decl(&decl)
                    .tail("\n<!-- epilog comment -->\n<?epilog-pi more data?>\n")
                    .bytes())
            },
        ),
        fx(
            "xml-cdata-split-around-terminator",
            "a CDATA section whose content holds ]]> is written as two sections, and a comment holds single hyphens",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<!-- a - b - c --><w:p><w:r><w:t><![CDATA[x]]]]><![CDATA[>y]]></w:t></w:r></w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-space-preserve-padded-text",
            "xml:space=\"preserve\" on text with leading, trailing and doubled spaces",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p><w:r><w:t xml:space=\"preserve\">  lead  and  trail  </w:t></w:r></w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-mixed-content-in-paragraph",
            "text between the runs of a paragraph, before the first and after the last",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p>before<w:r><w:t>in</w:t></w:r>between<w:r><w:t>in2</w:t></w:r>after</w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-text-node-200000-chars",
            "one text node of 200000 characters",
            OK,
            long_text,
        ),
        fx(
            "xml-attribute-value-100000-chars",
            "one attribute value of 100000 characters",
            OK,
            long_attribute,
        ),
        fx(
            "xml-gt-in-attribute-value",
            "a literal > inside a single-quoted attribute value, next to attributes with spaces around =",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p w:rsidR='00A1' w:rsidRPr = \"00B2\"><w:r><w:rPr><w:lang w:val='x > y'/></w:rPr><w:t>t</w:t></w:r></w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-ns-prefix-rebound-in-siblings",
            "the prefix q bound to urn:one in one paragraph, to urn:two in the next, and to urn:three in a run inside it",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p xmlns:q=\"urn:one\"><q:a q:x=\"1\"/></w:p><w:p xmlns:q=\"urn:two\"><q:a q:x=\"2\"><w:r xmlns:q=\"urn:three\"><q:b/></w:r></q:a></w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-depth-100",
            "elements nested 100 levels deep, under the depth limit",
            OK,
            || {
                let body = format!("{}<w:p/>{}", "<w:x>".repeat(97), "</w:x>".repeat(97));
                pkg(&Doc::new().body(&body).bytes())
            },
        ),
        fx(
            "opc-empty-body",
            "the document body holds nothing but its section properties",
            OK,
            || pkg(&Doc::new().body("").bytes()),
        ),
    ]
}
