//! The smallest valid package, and the pieces fixtures vary.

use super::deflate;
use super::zip::Entry;

/// The WordprocessingML namespace.
pub const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// The relationships namespace.
pub const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// The Strict WordprocessingML namespace.
pub const NS_W_STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
/// The Strict relationships namespace.
pub const NS_R_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
/// The usual XML declaration, with a line feed after it.
pub const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
/// Two paragraphs, the second with leading and trailing spaces that need `xml:space`.
pub const BODY: &str = "<w:p><w:r><w:t>Hello edge</w:t></w:r></w:p><w:p><w:r><w:t xml:space=\"preserve\"> second paragraph </w:t></w:r></w:p>";

/// `[Content_Types].xml` for the minimal package.
pub fn content_types() -> String {
    format!(
        "{DECL}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/><Override PartName=\"/docProps/app.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.extended-properties+xml\"/></Types>"
    )
}

/// `_rels/.rels` for the minimal package.
pub fn root_rels() -> String {
    format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/><Relationship Id=\"rId3\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties\" Target=\"docProps/app.xml\"/></Relationships>"
    )
}

fn core() -> String {
    format!(
        "{DECL}<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:creator>stetdoc testkit fixture builder</dc:creator></cp:coreProperties>"
    )
}

fn app() -> String {
    format!(
        "{DECL}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/extended-properties\"><Application>stetdoc-testkit-fixture-builder</Application></Properties>"
    )
}

/// A `word/document.xml`, with every part of its text under the caller's control.
#[derive(Clone, Debug)]
pub struct Doc {
    body: String,
    decl: String,
    root_attrs: String,
    prefix: String,
    tail: String,
}

impl Default for Doc {
    fn default() -> Doc {
        Doc::new()
    }
}

impl Doc {
    /// The default document: [`BODY`], the usual declaration, a trailing line feed.
    pub fn new() -> Doc {
        Doc {
            body: BODY.into(),
            decl: DECL.into(),
            root_attrs: String::new(),
            prefix: "w".into(),
            tail: "\n".into(),
        }
    }

    /// Replaces the body content.
    #[must_use]
    pub fn body(mut self, body: &str) -> Doc {
        self.body = body.into();
        self
    }

    /// Replaces everything before the root element.
    #[must_use]
    pub fn decl(mut self, decl: &str) -> Doc {
        self.decl = decl.into();
        self
    }

    /// Extra attributes on the root element, with a leading space.
    #[must_use]
    pub fn root_attrs(mut self, attrs: &str) -> Doc {
        self.root_attrs = attrs.into();
        self
    }

    /// Replaces the bytes after the root end tag.
    #[must_use]
    pub fn tail(mut self, tail: &str) -> Doc {
        self.tail = tail.into();
        self
    }

    /// The document as text.
    pub fn xml(&self) -> String {
        let p = &self.prefix;
        format!(
            "{}<{p}:document xmlns:{p}=\"{NS_W}\"{}><{p}:body>{}<{p}:sectPr/></{p}:body></{p}:document>{}",
            self.decl, self.root_attrs, self.body, self.tail
        )
    }

    /// The document as UTF-8 bytes.
    pub fn bytes(&self) -> Vec<u8> {
        self.xml().into_bytes()
    }
}

/// The five parts of the minimal package, with `word/document.xml` given as bytes.
pub fn entries_with(document: &[u8], rels: &str, content_types: &str) -> Vec<Entry> {
    vec![
        Entry::new("[Content_Types].xml", content_types),
        Entry::new("_rels/.rels", rels),
        Entry::new("word/document.xml", document),
        Entry::new("docProps/core.xml", core()),
        Entry::new("docProps/app.xml", app()),
    ]
}

/// The minimal package with the default document.
pub fn entries() -> Vec<Entry> {
    entries_with(&Doc::new().bytes(), &root_rels(), &content_types())
}

/// The minimal package with this `word/document.xml`.
pub fn entries_doc(document: &[u8]) -> Vec<Entry> {
    entries_with(document, &root_rels(), &content_types())
}

/// A part of `total` zero bytes whose compressed form is a hand-built deflate stream of about
/// `total / 1000` bytes, so that a gigabyte of zeros never exists in memory.
pub fn zeros_part(name: &str, total: u32) -> Entry {
    let matches = (total - 1) / 258;
    let rest = ((total - 1) % 258) as usize;
    let stream = deflate::run_stream(b"", 0, matches, &vec![0u8; rest]);
    Entry::new(name, Vec::new())
        .raw(&stream.deflated)
        .declared_usize(stream.inflated_len as u32)
        .crc(stream.crc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_document_is_the_documented_shape() {
        let xml = Doc::new().xml();
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<w:sectPr/></w:body></w:document>\n"));
        assert_eq!(entries().len(), 5);
    }

    #[test]
    fn a_zeros_part_has_the_size_asked_for() {
        for total in [1u32, 2, 259, 260, 1000, 1 << 20] {
            let z = crate::fixtures::zip::Zip::new(vec![zeros_part("z", total)]).build();
            let a = crate::fixtures::inspect::read(&z).unwrap();
            let data = crate::fixtures::inspect::extract(&z, &a.records[0]).unwrap();
            assert_eq!(data.len(), total as usize, "{total}");
            assert!(data.iter().all(|b| *b == 0), "{total}");
        }
    }
}
