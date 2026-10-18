//! Hostile fixtures about the XML in `word/document.xml`.

use super::parts::{DECL, Doc, NS_W, entries_doc};
use super::zip::Zip;
use super::{DOCUMENT_XML, Expect, Fixture, hostile_fixture as fx};

const MALFORMED: &str = "malformed_input";
const LIMIT: &str = "limit_exceeded";

fn pkg(document: &[u8]) -> Vec<u8> {
    Zip::new(entries_doc(document)).build()
}

fn run(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

fn laughs() -> String {
    let mut s = String::from("<!DOCTYPE w:document [<!ENTITY a \"aaaaaaaaaa\">");
    let mut prev = 'a';
    for next in ['b', 'c', 'd', 'e', 'f', 'g', 'h'] {
        s.push_str(&format!(
            "<!ENTITY {next} \"{}\">",
            format!("&{prev};").repeat(10)
        ));
        prev = next;
    }
    s.push_str("]>\n");
    s
}

/// The XML-level hostile fixtures.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "xml-billion-laughs",
            "nested entity declarations that expand to about 10 GB",
            Expect::xml(MALFORMED),
            "xml.doctype.entity_declaration",
            || {
                pkg(&Doc::new()
                    .decl(&format!("{DECL}{}", laughs()))
                    .body(&run("&h;"))
                    .bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-doctype-internal-subset-plain",
            "a DOCTYPE with an internal subset that declares no entities",
            Expect::xml(MALFORMED),
            "xml.doctype.internal_subset",
            || {
                let decl = format!("{DECL}<!DOCTYPE w:document [<!ELEMENT w:document ANY>]>\n");
                pkg(&Doc::new().decl(&decl).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-external-entity-file",
            "an external general entity that names a local file",
            Expect::xml(MALFORMED),
            "xml.doctype.entity_declaration",
            || {
                let decl = format!(
                    "{DECL}<!DOCTYPE w:document [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]>\n"
                );
                pkg(&Doc::new().decl(&decl).body(&run("&xxe;")).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-external-dtd-http",
            "a DOCTYPE with an external DTD URL, which must never be fetched",
            Expect::xml(MALFORMED),
            "xml.doctype.external_id",
            || {
                let decl =
                    format!("{DECL}<!DOCTYPE w:document SYSTEM \"http://127.0.0.1:9/evil.dtd\">\n");
                pkg(&Doc::new().decl(&decl).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-parameter-entity",
            "a parameter entity that loads an external file",
            Expect::xml(MALFORMED),
            "xml.doctype.entity_declaration",
            || {
                let decl = format!(
                    "{DECL}<!DOCTYPE w:document [<!ENTITY % p SYSTEM \"file:///etc/hostname\"> %p;]>\n"
                );
                pkg(&Doc::new().decl(&decl).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-undefined-entity-nbsp",
            "the HTML entity &nbsp;, which XML does not define",
            Expect::xml(MALFORMED),
            "xml.ref.undefined_entity",
            || pkg(&Doc::new().body(&run("a&nbsp;b")).bytes()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-truncated-mid-tag",
            "word/document.xml cut off inside the root",
            Expect::xml(MALFORMED),
            "xml.not_wellformed.core_part",
            || {
                let d = Doc::new().bytes();
                pkg(&d[..d.len() - 40])
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-unbalanced-tags",
            "an end tag that does not match its start tag",
            Expect::xml(MALFORMED),
            "xml.unbalanced_tags_by_count",
            || pkg(&Doc::new().body("<w:p><w:r><w:t>x</w:r></w:p>").bytes()),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-duplicate-attribute",
            "the same attribute twice on one element",
            Expect::xml(MALFORMED),
            "xml.attr.duplicate_attribute",
            || {
                pkg(&Doc::new().body("<w:p><w:r><w:t xml:space=\"preserve\" xml:space=\"default\">x</w:t></w:r></w:p>").bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-undeclared-prefix",
            "an element with an undeclared prefix",
            Expect::xml(MALFORMED),
            "xml.ns.undeclared_prefix",
            || {
                pkg(&Doc::new()
                    .body("<w:p><q:r><w:t>x</w:t></q:r></w:p>")
                    .bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-invalid-utf8",
            "invalid UTF-8 byte sequences in text",
            Expect::xml(MALFORMED),
            "xml.invalid_encoding_bytes",
            || {
                let d = Doc::new().body(&run("bad @@@@")).bytes();
                let at = d.windows(4).position(|w| w == b"@@@@").unwrap_or(0);
                let mut out = d[..at].to_vec();
                out.extend([0xFF, 0xFE, 0xC3, 0x28]);
                out.extend(&d[at + 4..]);
                pkg(&out)
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-nul-and-control-chars",
            "NUL, U+0001 and U+0008 written literally, which XML 1.0 forbids",
            Expect::xml(MALFORMED),
            "xml.invalid_xml_chars",
            || {
                pkg(&Doc::new()
                    .body(&run("nul\0 and \u{1}\u{8} control"))
                    .bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-char-ref-illegal",
            "numeric references to NUL, a control character, a lone surrogate and a code point above U+10FFFF",
            Expect::xml(MALFORMED),
            "xml.ref.numeric.invalid",
            || {
                pkg(&Doc::new()
                    .body(&run("&#0; &#x1; &#xD800; &#x110000;"))
                    .bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-deep-nesting-20000",
            "20000 levels of nested w:ins",
            Expect::xml(LIMIT),
            "xml.depth_ge_100",
            || {
                let body = format!(
                    "{}<w:r><w:t>deep</w:t></w:r>{}",
                    "<w:ins w:id=\"1\">".repeat(20_000),
                    "</w:ins>".repeat(20_000)
                );
                pkg(&Doc::new().body(&body).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-attributes-100000",
            "one element with 100000 distinct attributes",
            Expect::xml(LIMIT),
            "xml.element_with_ge_50_attrs",
            || {
                let attrs: Vec<String> = (0..100_000).map(|i| format!("a{i}=\"1\"")).collect();
                let body = format!("<w:p {}><w:r><w:t>x</w:t></w:r></w:p>", attrs.join(" "));
                pkg(&Doc::new().body(&body).bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-bom-and-utf16-declaration-conflict",
            "a UTF-8 BOM with a declaration that says UTF-16",
            Expect::xml(MALFORMED),
            "xml.decl_encoding_conflicts_with_bytes",
            || {
                let mut d = vec![0xEF, 0xBB, 0xBF];
                d.extend(
                    Doc::new()
                        .decl("<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n")
                        .bytes(),
                );
                pkg(&d)
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-xml11-declaration",
            "an XML 1.1 declaration",
            Expect::xml(MALFORMED),
            "xml.decl.version_not_1_0",
            || {
                pkg(&Doc::new()
                    .decl("<?xml version=\"1.1\" encoding=\"UTF-8\"?>\n")
                    .bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-two-root-elements",
            "content after the root element",
            Expect::xml(MALFORMED),
            "xml.epilog.junk",
            || {
                let mut d = Doc::new().xml();
                d.push_str(&format!("<w:extra xmlns:w=\"{NS_W}\"/>"));
                pkg(d.as_bytes())
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-text-before-root",
            "text before the declaration",
            Expect::xml(MALFORMED),
            "xml.junk_before_root",
            || {
                let mut d = b"junk".to_vec();
                d.extend(Doc::new().bytes());
                pkg(&d)
            },
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "xml-cdata-end-in-text",
            "`]]>` in the text of an element, which XML allows only at the end of a CDATA section",
            Expect::xml(MALFORMED),
            "xml.cdata_end_in_text",
            || pkg(&Doc::new().body(&run("a]]>b")).bytes()),
        )
        .in_parts(DOCUMENT_XML),
    ]
}
