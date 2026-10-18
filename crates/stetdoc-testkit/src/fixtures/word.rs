//! Minimal Word documents for one feature each, for the schema validator and the text model.
//!
//! Each function returns a complete package. The documents are small, well formed and
//! (apart from the deliberate defects named in each function's docs) valid against the
//! ECMA-376 Transitional schema, so a validator test can say exactly which finding a
//! document must produce. `clean`, `misordered` and `extensions` share one shape so that
//! two of them differ only in the one thing under test.

use super::parts::{DECL, NS_R, NS_R_STRICT, NS_W, NS_W_STRICT};
use super::zip::{Entry, Zip};
use super::{Fixture, word_fixture as fx};

const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const VML: &str = "urn:schemas-microsoft-com:vml";
const REL_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_TYPE_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const MAIN_CT: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml";

/// A part besides `word/document.xml`, with how the document relates to it.
struct Part {
    name: &'static str,
    content_type: &'static str,
    relationship: &'static str,
    xml: String,
}

/// A package of `word/document.xml` and its related parts.
pub(crate) struct Package {
    strict: bool,
    document: String,
    parts: Vec<Part>,
}

impl Package {
    pub(crate) fn new(document: String) -> Package {
        Package {
            strict: false,
            document,
            parts: Vec::new(),
        }
    }

    fn strict(mut self) -> Package {
        self.strict = true;
        self
    }

    pub(crate) fn part(
        mut self,
        name: &'static str,
        content_type: &'static str,
        relationship: &'static str,
        xml: String,
    ) -> Package {
        self.parts.push(Part {
            name,
            content_type,
            relationship,
            xml,
        });
        self
    }

    pub(crate) fn build(self) -> Vec<u8> {
        let rel = if self.strict {
            REL_TYPE_STRICT
        } else {
            REL_TYPE
        };
        let mut overrides =
            format!("<Override PartName=\"/word/document.xml\" ContentType=\"{MAIN_CT}\"/>");
        let mut doc_rels = String::new();
        for (i, p) in self.parts.iter().enumerate() {
            overrides.push_str(&format!(
                "<Override PartName=\"/{}\" ContentType=\"{}\"/>",
                p.name, p.content_type
            ));
            doc_rels.push_str(&format!(
                "<Relationship Id=\"rId{}\" Type=\"{rel}/{}\" Target=\"{}\"/>",
                i + 1,
                p.relationship,
                p.name.trim_start_matches("word/")
            ));
        }
        overrides.push_str("<Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/>");
        overrides.push_str("<Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/>");
        let content_types = format!(
            "{DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/>{overrides}</Types>"
        );
        let root_rels = format!(
            "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{rel}/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"{REL_TYPE}/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>"
        );
        let core = format!(
            "{DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:creator>stetdoc testkit fixture builder</dc:creator></cp:coreProperties>"
        );
        let app = format!(
            "{DECL}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>stetdoc-testkit-fixture-builder</Application></Properties>"
        );
        let mut entries = vec![
            Entry::new("[Content_Types].xml", content_types),
            Entry::new("_rels/.rels", root_rels),
        ];
        if !self.parts.is_empty() {
            let doc_rels = format!(
                "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{doc_rels}</Relationships>"
            );
            entries.push(Entry::new("word/_rels/document.xml.rels", doc_rels));
        }
        entries.push(Entry::new("word/document.xml", self.document));
        for p in self.parts {
            entries.push(Entry::new(p.name, p.xml));
        }
        entries.push(Entry::new("docProps/core.xml", core));
        entries.push(Entry::new("docProps/app.xml", app));
        Zip::new(entries).build()
    }
}

const SECT_PR: &str = "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr>";

fn document(strict: bool, root_attrs: &str, body: &str) -> String {
    let w = if strict { NS_W_STRICT } else { NS_W };
    let r = if strict { NS_R_STRICT } else { NS_R };
    format!(
        "{DECL}<w:document xmlns:w=\"{w}\" xmlns:r=\"{r}\"{root_attrs}><w:body>{body}{SECT_PR}</w:body></w:document>\n"
    )
}

fn comments_part() -> String {
    format!(
        "{DECL}<w:comments xmlns:w=\"{NS_W}\"><w:comment w:id=\"0\" w:author=\"Reviewer\" w:date=\"2026-01-01T00:00:00Z\" w:initials=\"R\"><w:p><w:r><w:t>A comment.</w:t></w:r></w:p></w:comment></w:comments>\n"
    )
}

fn styles_part() -> String {
    format!(
        "{DECL}<w:styles xmlns:w=\"{NS_W}\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/></w:style></w:styles>\n"
    )
}

const BORDERS: &str = "<w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/></w:tblBorders>";
const LOOK: &str = "<w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/>";

/// The shared shape: a paragraph with a commented run, a one-cell table and a paragraph.
fn model_body(misordered: bool) -> String {
    let ppr = if misordered {
        "<w:pPr><w:jc w:val=\"left\"/><w:pStyle w:val=\"Normal\"/></w:pPr>"
    } else {
        "<w:pPr><w:pStyle w:val=\"Normal\"/><w:jc w:val=\"left\"/></w:pPr>"
    };
    let tbl_pr = if misordered {
        format!("<w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/>{LOOK}{BORDERS}</w:tblPr>")
    } else {
        format!("<w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/>{BORDERS}{LOOK}</w:tblPr>")
    };
    format!(
        "<w:p>{ppr}<w:commentRangeStart w:id=\"0\"/><w:r><w:t>Hello model</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p><w:tbl>{tbl_pr}<w:tblGrid><w:gridCol w:w=\"4680\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"4680\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"
    )
}

fn with_comment_and_styles(document: String) -> Package {
    Package::new(document)
        .part(
            "word/comments.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
            "comments",
            comments_part(),
        )
        .part(
            "word/styles.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml",
            "styles",
            styles_part(),
        )
}

/// One of each: a paragraph (with `pPr`, a commented run and its text), a table (`tblPr`,
/// `tblGrid`, `tr`, `tc`, a paragraph), a `sectPr`, `comments.xml` with one comment and a
/// `styles.xml` that defines the paragraph style. Every child is in schema order, so the
/// document validates against the ECMA-376 Transitional schema without findings.
pub fn clean() -> Vec<u8> {
    with_comment_and_styles(document(false, "", &model_body(false))).build()
}

/// The same package as [`clean`] with two ordering defects that real producers write:
/// `tblLook` before `tblBorders` in `tblPr` (python-docx's shape) and `jc` before `pStyle`
/// in `pPr`. The XML is well formed and the package opens, so L1 holds. A schema validator
/// must report exactly these two child-order findings and no others.
pub fn misordered() -> Vec<u8> {
    with_comment_and_styles(document(false, "", &model_body(true))).build()
}

/// A paragraph carrying `w14:paraId` and `w14:textId`, with `mc:Ignorable="w14"` on the root
/// when `ignorable` is true and without it when false. The ECMA-376 Transitional XSD alone
/// rejects these attributes in both documents, but a consumer that understands `w14` reads them
/// whether or not the root lists `w14` as ignorable (ECMA-376 Part 3), so a schema module that
/// knows the Office extension tables reports nothing for either. That the prefix must be listed
/// so that Word 2007 can skip it is a Word compatibility rule for a `check` module, and the
/// document without `mc:Ignorable` is the input for it.
pub fn extensions(ignorable: bool) -> Vec<u8> {
    let ignorable = if ignorable {
        " mc:Ignorable=\"w14\""
    } else {
        ""
    };
    let root = format!(" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\"{ignorable}");
    let body = "<w:p w14:paraId=\"1A2B3C4D\" w14:textId=\"77777777\"><w:r><w:t>extension attributes</w:t></w:r></w:p>";
    Package::new(document(false, &root, body)).build()
}

/// A run holding `mc:AlternateContent`: a `Choice` that requires `wps` (a drawing with a text
/// box) and a `Fallback` (a VML picture with the same text). The consumer keeps one branch;
/// the other is never validated.
pub fn alternate_content() -> Vec<u8> {
    let root = format!(
        " xmlns:mc=\"{MC}\" xmlns:wp=\"{WP}\" xmlns:a=\"{DML}\" xmlns:wps=\"{WPS}\" xmlns:v=\"{VML}\""
    );
    let choice = "<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"1828800\" cy=\"914400\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:docPr id=\"1\" name=\"Text Box 1\"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"1828800\" cy=\"914400\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>box text</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing>";
    let fallback = "<w:pict><v:shape id=\"TextBox1\" style=\"width:144pt;height:72pt\"><v:textbox><w:txbxContent><w:p><w:r><w:t>box text</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict>";
    let body = format!(
        "<w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\">{choice}</mc:Choice><mc:Fallback>{fallback}</mc:Fallback></mc:AlternateContent></w:r></w:p>"
    );
    Package::new(document(false, &root, &body)).build()
}

/// A `w:smartTag` around a run. With `strict` the root and the relationship types use the
/// ISO Strict namespaces instead of the Transitional ones.
pub fn smart_tag(strict: bool) -> Vec<u8> {
    let body = "<w:p><w:smartTag w:uri=\"urn:schemas-microsoft-com:office:smarttags\" w:element=\"place\"><w:r><w:t>Paris</w:t></w:r></w:smartTag></w:p>";
    let package = Package::new(document(strict, "", body));
    if strict { package.strict() } else { package }.build()
}

/// The registry entries for these documents. Every one opens, reads and round-trips.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "word-clean",
            "one of each paragraph, run, table, section and comment element, all in schema order",
            clean,
        ),
        fx(
            "word-misordered",
            "the clean document with tblLook before tblBorders and jc before pStyle",
            misordered,
        ),
        fx(
            "word-extensions-ignorable",
            "w14:paraId and w14:textId with mc:Ignorable=\"w14\" on the root",
            || extensions(true),
        ),
        fx(
            "word-extensions-not-ignorable",
            "w14:paraId and w14:textId without mc:Ignorable",
            || extensions(false),
        ),
        fx(
            "word-alternate-content-wps",
            "mc:AlternateContent with a Choice that requires wps and a VML Fallback",
            alternate_content,
        ),
        fx("word-smart-tag", "a w:smartTag around a run", || {
            smart_tag(false)
        }),
        fx(
            "word-smart-tag-strict",
            "a w:smartTag around a run in the ISO Strict namespace",
            || smart_tag(true),
        ),
    ]
}
