//! Edge fixtures about the XML in `word/document.xml`: valid documents written in unusual ways.

use super::parts::{DECL, Doc, NS_R, NS_R_STRICT, NS_W, NS_W_STRICT, entries_doc};
use super::zip::Zip;
use super::{Expect, Fixture, edge_fixture as fx};

const OK: Expect = Expect::Ok;

fn pkg(document: &[u8]) -> Vec<u8> {
    Zip::new(entries_doc(document)).build()
}

fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16be(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

fn run(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

const DECL_UTF16: &str = "<?xml version=\"1.0\" encoding=\"UTF-16\" standalone=\"yes\"?>\n";

/// The XML-level edge fixtures.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "xml-bom-utf8",
            "a UTF-8 byte order mark before the declaration of word/document.xml",
            OK,
            || {
                let mut d = vec![0xEF, 0xBB, 0xBF];
                d.extend(Doc::new().bytes());
                pkg(&d)
            },
        ),
        fx(
            "xml-utf16le-bom",
            "word/document.xml encoded as UTF-16LE with a BOM and encoding=\"UTF-16\"",
            OK,
            || {
                let mut d = vec![0xFF, 0xFE];
                d.extend(utf16le(&Doc::new().decl(DECL_UTF16).xml()));
                pkg(&d)
            },
        ),
        fx(
            "xml-utf16be-bom",
            "word/document.xml encoded as UTF-16BE with a BOM",
            OK,
            || {
                let mut d = vec![0xFE, 0xFF];
                d.extend(utf16be(&Doc::new().decl(DECL_UTF16).xml()));
                pkg(&d)
            },
        ),
        fx("xml-no-declaration", "no XML declaration", OK, || {
            pkg(&Doc::new().decl("").bytes())
        }),
        fx(
            "xml-declaration-single-quotes",
            "a declaration with single quotes, as lxml writes it",
            OK,
            || {
                pkg(&Doc::new()
                    .decl("<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n")
                    .bytes())
            },
        ),
        fx(
            "xml-declaration-no-standalone-lowercase-utf8",
            "lower-case encoding name and no standalone",
            OK,
            || {
                pkg(&Doc::new()
                    .decl("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n")
                    .bytes())
            },
        ),
        fx(
            "xml-declaration-crlf-after",
            "CRLF after the declaration and after the root end tag, as Word writes",
            OK,
            || {
                pkg(&Doc::new()
                    .decl(&DECL.replace('\n', "\r\n"))
                    .tail("\r\n")
                    .bytes())
            },
        ),
        fx(
            "xml-crlf-everywhere",
            "CRLF line endings between every element",
            OK,
            || {
                let body = "\r\n<w:p>\r\n<w:r>\r\n<w:t>line</w:t>\r\n</w:r>\r\n</w:p>\r\n";
                pkg(&Doc::new()
                    .body(body)
                    .decl(&DECL.replace('\n', "\r\n"))
                    .tail("\r\n")
                    .bytes())
            },
        ),
        fx(
            "xml-bare-cr-and-lf-mixed",
            "bare CR and bare LF mixed between elements",
            OK,
            || {
                pkg(&Doc::new()
                    .body("\r<w:p>\n<w:r>\r<w:t>mixed</w:t>\n</w:r>\r</w:p>\n")
                    .bytes())
            },
        ),
        fx(
            "xml-cr-in-text-content",
            "a literal CR and a CRLF inside w:t text, which a parser normalises to LF",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p><w:r><w:t xml:space=\"preserve\">a\rb\r\nc</w:t></w:r></w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-no-trailing-newline",
            "no bytes after the root end tag",
            OK,
            || pkg(&Doc::new().tail("").bytes()),
        ),
        fx(
            "xml-pretty-printed",
            "indentation whitespace between elements",
            OK,
            || {
                let body = "\n    <w:p>\n        <w:r>\n            <w:t>indented</w:t>\n        </w:r>\n    </w:p>\n  ";
                pkg(&Doc::new().body(body).bytes())
            },
        ),
        fx(
            "xml-char-refs-numeric",
            "numeric character references: decimal, hex with upper and lower digits, astral, ASCII, NBSP",
            OK,
            || {
                let t = run(
                    "nbsp&#xA0;hex-upper &#xA9; hex-lower &#xa9; dec &#169; astral &#x1F600; ascii-A &#65; lt-as-ref &#60;",
                );
                pkg(&Doc::new().body(&t).bytes())
            },
        ),
        fx(
            "xml-named-entities-all",
            "all five predefined entities in text",
            OK,
            || {
                pkg(&Doc::new()
                    .body(&run(
                        "&amp; &lt; &gt; &quot; &apos; and raw &gt; unescaped &gt;&gt;",
                    ))
                    .bytes())
            },
        ),
        fx(
            "xml-literal-vs-escaped-quotes",
            "literal and escaped quote characters and a literal greater-than in text",
            OK,
            || {
                pkg(&Doc::new().body(&run("\"literal dquote\" 'literal apos' &quot;escaped&quot; &apos;escaped&apos; &gt; >")).bytes())
            },
        ),
        fx(
            "xml-literal-nbsp-char",
            "literal NBSP, soft hyphen, U+FEFF, U+2028 and an astral character",
            OK,
            || {
                pkg(&Doc::new().body(&run("literal\u{a0}nbsp and soft\u{ad}hyphen and zwnbsp\u{feff} and line-sep\u{2028} and astral \u{1F600}")).bytes())
            },
        ),
        fx(
            "xml-cdata-in-text",
            "a CDATA section inside w:t",
            OK,
            || {
                pkg(&Doc::new()
                    .body(&run("<![CDATA[CDATA <b>text</b> & more]]>"))
                    .bytes())
            },
        ),
        fx(
            "xml-comments-and-pis",
            "comments and processing instructions in the prolog and the body",
            OK,
            || {
                let body = "<!-- body comment --><w:p><?stetdoc keep=\"me\"?><w:r><w:t>x</w:t></w:r><!--c2--></w:p>";
                let decl = format!(
                    "{DECL}<!-- prolog comment -->\n<?mso-application progid=\"Word.Document\"?>\n"
                );
                pkg(&Doc::new().body(body).decl(&decl).bytes())
            },
        ),
        fx(
            "xml-single-quoted-attributes",
            "single-quoted attribute values, including on the root",
            OK,
            || {
                let attrs = format!(" xmlns:r='{NS_R}'");
                pkg(&Doc::new()
                    .body("<w:p><w:r><w:t xml:space='preserve'> single </w:t></w:r></w:p>")
                    .root_attrs(&attrs)
                    .bytes())
            },
        ),
        fx(
            "xml-mixed-quotes-and-spacing",
            "extra spaces and a newline inside tags, spaces around =, a space before > and in an end tag",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p  ><w:r\n><w:t  xml:space = \"preserve\" >spaced</w:t ></w:r></w:p>")
                    .bytes())
            },
        ),
        fx(
            "xml-attr-newline-and-tab-values",
            "attribute values with numeric references to LF and tab, and a literal LF",
            OK,
            || {
                let body = "<w:p><w:bookmarkStart w:id=\"0\" w:name=\"a&#10;b&#9;c\"/><w:r><w:t>x</w:t></w:r><w:bookmarkEnd w:id=\"0\"/></w:p><w:p><w:bookmarkStart w:id=\"1\" w:name=\"literal\nnewline\"/><w:bookmarkEnd w:id=\"1\"/></w:p>";
                pkg(&Doc::new().body(body).bytes())
            },
        ),
        fx(
            "xml-empty-element-styles",
            "an empty element written four ways",
            OK,
            || {
                let body = "<w:p><w:r><w:t></w:t></w:r><w:r><w:t/></w:r><w:r><w:t /></w:r><w:r></w:r></w:p>";
                pkg(&Doc::new().body(body).bytes())
            },
        ),
        fx(
            "xml-default-namespace-for-w",
            "WordprocessingML bound as the default namespace, with no w: prefix",
            OK,
            || {
                let d = format!(
                    "{DECL}<document xmlns=\"{NS_W}\"><body><p><r><t>default ns</t></r></p><sectPr/></body></document>\n"
                );
                pkg(d.as_bytes())
            },
        ),
        fx(
            "xml-odd-prefix-ns0",
            "the prefix ns0 for the main namespace, as XmlSerializer writes it",
            OK,
            || {
                let d = format!(
                    "{DECL}<ns0:document xmlns:ns0=\"{NS_W}\"><ns0:body><ns0:p><ns0:r><ns0:t>ns0 prefix</ns0:t></ns0:r></ns0:p><ns0:sectPr/></ns0:body></ns0:document>\n"
                );
                pkg(d.as_bytes())
            },
        ),
        fx(
            "xml-ns-declared-below-root",
            "namespace declarations on non-root elements, including a redundant redeclaration of w",
            OK,
            || {
                let d = format!(
                    "{DECL}<w:document xmlns:w=\"{NS_W}\"><w:body><w:p xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" w14:paraId=\"1A2B3C4D\"><w:r xmlns:r=\"{NS_R}\"><w:t>below root</w:t></w:r></w:p><w:p xmlns:w=\"{NS_W}\"><w:r><w:t>redundant redeclare</w:t></w:r></w:p><w:sectPr/></w:body></w:document>\n"
                );
                pkg(d.as_bytes())
            },
        ),
        fx(
            "xml-ns-prefix-rebound",
            "the prefix x rebound to a different URI below the root",
            OK,
            || {
                let d = format!(
                    "{DECL}<w:document xmlns:w=\"{NS_W}\" xmlns:x=\"urn:a\"><w:body><x:thing xmlns:x=\"urn:b\"><w:p><w:r><w:t>rebound</w:t></w:r></w:p></x:thing><w:sectPr/></w:body></w:document>\n"
                );
                pkg(d.as_bytes())
            },
        ),
        fx(
            "xml-mc-alternate-content-textbox",
            "mc:AlternateContent with a wps text box choice and a VML fallback with the same text, and mc:Ignorable on the root",
            OK,
            || {
                let body = "<w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><wp:extent cx=\"100\" cy=\"100\"/><wp:docPr id=\"1\" name=\"Text Box 1\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:txbx><w:txbxContent><w:p><w:r><w:t>box text</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape xmlns:v=\"urn:schemas-microsoft-com:vml\"><v:textbox><w:txbxContent><w:p><w:r><w:t>box text</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r></w:p>";
                let attrs = " xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" mc:Ignorable=\"w14\"";
                pkg(&Doc::new().body(body).root_attrs(attrs).bytes())
            },
        ),
        fx(
            "xml-strict-ooxml",
            "Strict OOXML namespaces in the main part and in the officeDocument relationship type",
            OK,
            || {
                use super::parts::{content_types, entries_with, root_rels};
                let d = format!(
                    "{DECL}<w:document xmlns:w=\"{NS_W_STRICT}\" xmlns:r=\"{NS_R_STRICT}\"><w:body><w:p><w:r><w:t>Strict OOXML</w:t></w:r></w:p><w:sectPr/></w:body></w:document>\n"
                );
                let rels = root_rels().replace(
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument",
                &format!("{NS_R_STRICT}/officeDocument"),
            );
                Zip::new(entries_with(d.as_bytes(), &rels, &content_types())).build()
            },
        ),
        fx(
            "xml-whitespace-only-runs",
            "whitespace-only w:t, with and without xml:space=\"preserve\"",
            OK,
            || {
                let body = "<w:p><w:r><w:t xml:space=\"preserve\"> </w:t></w:r><w:r><w:t xml:space=\"preserve\">   </w:t></w:r><w:r><w:t> </w:t></w:r></w:p>";
                pkg(&Doc::new().body(body).bytes())
            },
        ),
        fx(
            "xml-long-single-line",
            "one 120 KB line with 5000 runs in a paragraph",
            OK,
            || {
                let body = format!("<w:p>{}</w:p>", "<w:r><w:t>x</w:t></w:r>".repeat(5000));
                pkg(&Doc::new().body(&body).bytes())
            },
        ),
        fx(
            "xml-many-attributes-50",
            "an element with 100 attributes: 50 namespace declarations and 50 prefixed attributes",
            OK,
            || {
                let attrs: Vec<String> = (0..50)
                    .map(|i| format!("xmlns:n{i}=\"urn:n{i}\" n{i}:a=\"v\""))
                    .collect();
                let body = format!("<w:p {}><w:r><w:t>attrs</w:t></w:r></w:p>", attrs.join(" "));
                pkg(&Doc::new().body(&body).bytes())
            },
        ),
        fx(
            "xml-unusual-bytes-in-text",
            "a literal tab, double spaces and a zero-width space in text",
            OK,
            || {
                pkg(&Doc::new()
                    .body(&run("tab\there  double  space \u{200b} zero-width"))
                    .bytes())
            },
        ),
        fx(
            "xml-mc-ignorable-undeclared-prefix",
            "mc:Ignorable on the root lists w14, wp14 and an unknown zz and declares none of them, as python-docx leaves a template's root; Word refuses it, and an edit declares w14 and wp14 and drops zz (decision 0014)",
            OK,
            || {
                let attrs = " xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"w14 wp14 zz\"";
                pkg(&Doc::new().root_attrs(attrs).bytes())
            },
        ),
        fx(
            "xml-attribute-order-varied",
            "mc:Ignorable placed before the namespace declaration of its own prefix",
            OK,
            || {
                let body = "<w:p w:rsidR=\"00AB12CD\" w:rsidP=\"00112233\"><w:pPr><w:jc w:val=\"left\"/></w:pPr><w:r w:rsidRPr=\"00FFFFFF\"><w:t>a</w:t></w:r></w:p>";
                let attrs = " xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" mc:Ignorable=\"w14\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"";
                pkg(&Doc::new().body(body).root_attrs(attrs).bytes())
            },
        ),
        fx(
            "xml-cdata-end-written-in-text",
            "`]]&gt;` in the text of an element, the way to write `]]>` that XML allows, next to a CDATA section that ends with it",
            OK,
            || {
                pkg(&Doc::new()
                    .body("<w:p><w:r><w:t>a]]&gt;b<![CDATA[c]]>d</w:t></w:r></w:p>")
                    .bytes())
            },
        ),
    ]
}
