//! Documents for the tests of the index, the projection and what is built on them: a package from the XML of
//! its parts, opened as a state. `projection` is allowed to depend on `index`, so its tests use this file.

use crate::api::limits::Limits;
use crate::state::State;
use stetdoc_testkit::fixtures::{
    parts,
    zip::{Entry, Zip},
};

pub(crate) const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const TYPES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A paragraph with one plain run.
pub(crate) fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

/// A paragraph with these XML children.
pub(crate) fn px(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

/// A run of text.
pub(crate) fn r(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

/// An insertion by `author`.
pub(crate) fn ins(id: i64, author: &str, text: &str) -> String {
    format!(
        "<w:ins w:id=\"{id}\" w:author=\"{author}\" w:date=\"2026-01-0{}T10:00:00Z\">{}</w:ins>",
        id % 9 + 1,
        r(text)
    )
}

/// A deletion by `author`.
pub(crate) fn del(id: i64, author: &str, text: &str) -> String {
    format!(
        "<w:del w:id=\"{id}\" w:author=\"{author}\" w:date=\"2026-01-0{}T10:00:00Z\"><w:r><w:delText xml:space=\"preserve\">{text}</w:delText></w:r></w:del>",
        id % 9 + 1
    )
}

/// A document under construction: the XML of the body and of each related part.
pub(crate) struct Fixture {
    body: String,
    related: Vec<(String, String, String)>,
    rels: String,
    sect: String,
}

impl Fixture {
    /// A document with this body, in the `w:body` and followed by the final section properties.
    pub(crate) fn new(body: &str) -> Fixture {
        Fixture {
            body: body.to_string(),
            related: Vec::new(),
            rels: String::new(),
            sect: String::new(),
        }
    }

    /// A part related from the main part by the type `kind`, with this XML.
    pub(crate) fn part(mut self, kind: &str, name: &str, xml: String) -> Fixture {
        self.related.push((kind.to_string(), name.to_string(), xml));
        self
    }

    /// The styles: the default styles and `inner` after them.
    pub(crate) fn styles(self, inner: &str) -> Fixture {
        let xml = format!(
            "<w:styles xmlns:w=\"{W}\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>\
             <w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"><w:name w:val=\"Default Paragraph Font\"/></w:style>{inner}</w:styles>"
        );
        self.part("styles", "word/styles.xml", xml)
    }

    /// A numbering part with one decimal list, `numId` 1, and one bullet list, `numId` 2.
    pub(crate) fn numbering(self) -> Fixture {
        let xml = format!(
            "<w:numbering xmlns:w=\"{W}\">\
             <w:abstractNum w:abstractNumId=\"0\"><w:multiLevelType w:val=\"multilevel\"/>\
             <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/></w:lvl>\
             <w:lvl w:ilvl=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"lowerLetter\"/><w:lvlText w:val=\"(%2)\"/></w:lvl></w:abstractNum>\
             <w:abstractNum w:abstractNumId=\"1\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"bullet\"/><w:lvlText w:val=\"\u{f0b7}\"/></w:lvl></w:abstractNum>\
             <w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>"
        );
        self.part("numbering", "word/numbering.xml", xml)
    }

    /// The footnotes: the two separator notes Word writes, then `inner`.
    pub(crate) fn footnotes(self, inner: &str) -> Fixture {
        let xml = format!(
            "<w:footnotes xmlns:w=\"{W}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>\
             <w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{inner}</w:footnotes>"
        );
        self.part("footnotes", "word/footnotes.xml", xml)
    }

    /// The endnotes, with the separators.
    pub(crate) fn endnotes(self, inner: &str) -> Fixture {
        let xml = format!(
            "<w:endnotes xmlns:w=\"{W}\"><w:endnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:endnote>{inner}</w:endnotes>"
        );
        self.part("endnotes", "word/endnotes.xml", xml)
    }

    /// The comments.
    pub(crate) fn comments(self, inner: &str) -> Fixture {
        let xml = format!("<w:comments xmlns:w=\"{W}\">{inner}</w:comments>");
        self.part("comments", "word/comments.xml", xml)
    }

    /// The header part number `n`, related as `rIdH{n}`.
    pub(crate) fn header(mut self, n: u32, inner: &str) -> Fixture {
        let xml = format!("<w:hdr xmlns:w=\"{W}\">{inner}</w:hdr>");
        self.rels.push_str(&format!(
            "<Relationship Id=\"rIdH{n}\" Type=\"{TYPES}/header\" Target=\"header{n}.xml\"/>"
        ));
        self.related
            .push((String::new(), format!("word/header{n}.xml"), xml));
        self
    }

    /// The footer part number `n`, related as `rIdF{n}`.
    pub(crate) fn footer(mut self, n: u32, inner: &str) -> Fixture {
        let xml = format!("<w:ftr xmlns:w=\"{W}\">{inner}</w:ftr>");
        self.rels.push_str(&format!(
            "<Relationship Id=\"rIdF{n}\" Type=\"{TYPES}/footer\" Target=\"footer{n}.xml\"/>"
        ));
        self.related
            .push((String::new(), format!("word/footer{n}.xml"), xml));
        self
    }

    /// An external hyperlink target.
    pub(crate) fn external(mut self, id: &str, url: &str) -> Fixture {
        self.rels.push_str(&format!(
            "<Relationship Id=\"{id}\" Type=\"{TYPES}/hyperlink\" Target=\"{url}\" TargetMode=\"External\"/>"
        ));
        self
    }

    /// The content of the final `w:sectPr`.
    pub(crate) fn sect(mut self, inner: &str) -> Fixture {
        self.sect = inner.to_string();
        self
    }

    /// The package, opened.
    pub(crate) fn state(&self) -> State {
        State::open(self.bytes(), &Limits::default()).unwrap()
    }

    /// The package, as the bytes of a .docx.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        let document = format!(
            "<w:document xmlns:w=\"{W}\" xmlns:r=\"{R}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
             xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
             xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
             xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\" \
             xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" \
             xmlns:v=\"urn:schemas-microsoft-com:vml\" \
             xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\">\
             <w:body>{}<w:sectPr>{}</w:sectPr></w:body></w:document>",
            self.body, self.sect
        );
        let mut entries = parts::entries_doc(document.as_bytes());
        let mut rels = self.rels.clone();
        for (i, (kind, name, xml)) in self.related.iter().enumerate() {
            entries.push(Entry::new(name.as_str(), xml.as_str()));
            if !kind.is_empty() {
                let target = name.strip_prefix("word/").unwrap_or(name);
                rels.push_str(&format!(
                    "<Relationship Id=\"rId{}\" Type=\"{TYPES}/{kind}\" Target=\"{target}\"/>",
                    i + 1
                ));
            }
        }
        entries.push(Entry::new(
            "word/_rels/document.xml.rels",
            format!(
                "<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"
            ),
        ));
        Zip::new(entries).build()
    }
}
