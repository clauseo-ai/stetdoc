//! Small packages with comments, built by hand, for the tests of the reader and the audit.

use crate::api::limits::Limits;
use crate::schema::PartKind;
use crate::state::State;
use stetdoc_testkit::fixtures::{
    parts::{self, DECL, NS_W},
    zip::{Entry, Zip},
};

pub(crate) const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
pub(crate) const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
pub(crate) const CID: &str = "http://schemas.microsoft.com/office/word/2016/wordml/cid";
pub(crate) const CEX: &str = "http://schemas.microsoft.com/office/word/2018/wordml/cex";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const CT: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml.";

/// A package under construction: a body and the comment parts that exist.
pub(crate) struct Pkg {
    strict: bool,
    body: String,
    parts: Vec<(PartKind, &'static str, &'static str, String)>,
}

impl Pkg {
    /// A package whose body is `body`.
    pub(crate) fn new(body: &str) -> Pkg {
        Pkg {
            strict: false,
            body: body.to_owned(),
            parts: Vec::new(),
        }
    }

    /// Makes the document Strict: its root is in the Strict namespace.
    pub(crate) fn strict(mut self) -> Pkg {
        self.strict = true;
        self
    }

    /// Adds `word/styles.xml` with this content.
    pub(crate) fn styles(self, xml: &str) -> Pkg {
        let xml = format!("{DECL}<w:styles xmlns:w=\"{}\">{xml}</w:styles>", self.ns());
        self.with(PartKind::Styles, "word/styles.xml", "styles+xml", xml)
    }

    /// Adds `word/numbering.xml` with this content.
    pub(crate) fn numbering(self, xml: &str) -> Pkg {
        let xml = format!(
            "{DECL}<w:numbering xmlns:w=\"{}\">{xml}</w:numbering>",
            self.ns()
        );
        self.with(
            PartKind::Numbering,
            "word/numbering.xml",
            "numbering+xml",
            xml,
        )
    }

    fn ns(&self) -> &'static str {
        if self.strict {
            "http://purl.oclc.org/ooxml/wordprocessingml/main"
        } else {
            NS_W
        }
    }

    fn with(mut self, kind: PartKind, name: &'static str, ct: &'static str, xml: String) -> Pkg {
        self.parts.push((kind, name, ct, xml));
        self
    }

    /// Adds `word/comments.xml` with these `w:comment` elements.
    pub(crate) fn comments(self, items: &[String]) -> Pkg {
        let xml = format!(
            "{DECL}<w:comments xmlns:w=\"{}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" mc:Ignorable=\"w14\">{}</w:comments>",
            self.ns(),
            items.concat()
        );
        self.with(PartKind::Comments, "word/comments.xml", "comments+xml", xml)
    }

    /// Adds `word/commentsExtended.xml` with these `w15:commentEx` rows.
    pub(crate) fn extended(self, rows: &[String]) -> Pkg {
        let xml = format!(
            "{DECL}<w15:commentsEx xmlns:w15=\"{W15}\" xmlns:mc=\"{MC}\" mc:Ignorable=\"w15\">{}</w15:commentsEx>",
            rows.concat()
        );
        self.with(
            PartKind::CommentsExtended,
            "word/commentsExtended.xml",
            "commentsExtended+xml",
            xml,
        )
    }

    /// Adds `word/commentsExtended.xml` in the namespace that Word 2013's preview wrote `w15` in.
    pub(crate) fn extended_preview(self, rows: &[String]) -> Pkg {
        let xml = format!(
            "{DECL}<w15:commentsEx xmlns:w15=\"http://schemas.microsoft.com/office/word/2010/11/wordml\">{}</w15:commentsEx>",
            rows.concat()
        );
        self.with(
            PartKind::CommentsExtended,
            "word/commentsExtended.xml",
            "commentsExtended+xml",
            xml,
        )
    }

    /// Adds `word/commentsIds.xml` with these `w16cid:commentId` rows.
    pub(crate) fn ids(self, rows: &[String]) -> Pkg {
        let xml = format!(
            "{DECL}<w16cid:commentsIds xmlns:w16cid=\"{CID}\" xmlns:mc=\"{MC}\" mc:Ignorable=\"w16cid\">{}</w16cid:commentsIds>",
            rows.concat()
        );
        self.with(
            PartKind::CommentsIds,
            "word/commentsIds.xml",
            "commentsIds+xml",
            xml,
        )
    }

    /// Adds `word/commentsExtensible.xml` with these `w16cex:commentExtensible` rows.
    pub(crate) fn extensible(self, rows: &[String]) -> Pkg {
        let xml = format!(
            "{DECL}<w16cex:commentsExtensible xmlns:w16cex=\"{CEX}\" xmlns:mc=\"{MC}\" mc:Ignorable=\"w16cex\">{}</w16cex:commentsExtensible>",
            rows.concat()
        );
        self.with(
            PartKind::CommentsExtensible,
            "word/commentsExtensible.xml",
            "commentsExtensible+xml",
            xml,
        )
    }

    /// Adds `word/footnotes.xml` with these `w:footnote` elements after the two separators Word writes.
    pub(crate) fn footnotes(self, items: &str) -> Pkg {
        let xml = format!(
            "{DECL}<w:footnotes xmlns:w=\"{}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{items}</w:footnotes>",
            self.ns()
        );
        self.with(
            PartKind::Footnotes,
            "word/footnotes.xml",
            "footnotes+xml",
            xml,
        )
    }

    /// Adds `word/endnotes.xml`, the same.
    pub(crate) fn endnotes(self, items: &str) -> Pkg {
        let xml = format!(
            "{DECL}<w:endnotes xmlns:w=\"{}\"><w:endnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:endnote><w:endnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:endnote>{items}</w:endnotes>",
            self.ns()
        );
        self.with(PartKind::Endnotes, "word/endnotes.xml", "endnotes+xml", xml)
    }

    /// Adds `word/people.xml` with these authors.
    pub(crate) fn people(self, authors: &[&str]) -> Pkg {
        let rows: String = authors
            .iter()
            .map(|a| format!("<w15:person w15:author=\"{a}\"><w15:presenceInfo w15:providerId=\"None\" w15:userId=\"{a}\"/></w15:person>"))
            .collect();
        let xml = format!("{DECL}<w15:people xmlns:w15=\"{W15}\">{rows}</w15:people>");
        self.with(PartKind::People, "word/people.xml", "people+xml", xml)
    }

    /// The package as bytes. `related` lists the parts that the main part has a relationship to: a
    /// part in the package that is not in it is an unrelated part.
    pub(crate) fn bytes_related(&self, related: impl Fn(PartKind) -> bool) -> Vec<u8> {
        let doc = format!(
            "{DECL}<w:document xmlns:w=\"{}\"><w:body>{}<w:sectPr/></w:body></w:document>",
            self.ns(),
            self.body
        );
        let mut types = String::new();
        let mut rels = String::new();
        for (i, (kind, name, ct, _)) in self.parts.iter().enumerate() {
            types.push_str(&format!(
                "<Override PartName=\"/{name}\" ContentType=\"{CT}{ct}\"/>"
            ));
            if related(*kind) {
                rels.push_str(&format!(
                    "<Relationship Id=\"rId{}\" Type=\"{}\" Target=\"{}\"/>",
                    i + 1,
                    kind.relationship_type(),
                    name.trim_start_matches("word/")
                ));
            }
        }
        let content_types = parts::content_types().replace("</Types>", &format!("{types}</Types>"));
        let mut entries = parts::entries_with(doc.as_bytes(), &parts::root_rels(), &content_types);
        entries.push(Entry::new(
            "word/_rels/document.xml.rels",
            format!(
                "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>"
            ),
        ));
        for (_, name, _, xml) in &self.parts {
            entries.push(Entry::new(*name, xml.as_str()));
        }
        Zip::new(entries).build()
    }

    /// The package as bytes, with every part related.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.bytes_related(|_| true)
    }

    /// The package opened as a state.
    pub(crate) fn open(&self) -> State {
        State::open(self.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("the fixture opens: {e}"))
    }

    /// The package opened as a state, with only some parts related to the main part.
    pub(crate) fn open_related(&self, related: impl Fn(PartKind) -> bool) -> State {
        State::open(self.bytes_related(related), &Limits::default())
            .unwrap_or_else(|e| panic!("the fixture opens: {e}"))
    }
}

/// A `w:comment` whose paragraphs have these `paraId`s (`None` for a paragraph with none).
pub(crate) fn comment(id: i64, author: &str, paras: &[Option<&str>], text: &str) -> String {
    let ps: String = paras
        .iter()
        .map(|p| {
            let attr = p.map_or_else(String::new, |p| {
                format!(" w14:paraId=\"{p}\" w14:textId=\"77777777\"")
            });
            format!("<w:p{attr}><w:r><w:t>{text}</w:t></w:r></w:p>")
        })
        .collect();
    format!(
        "<w:comment w:id=\"{id}\" w:author=\"{author}\" w:date=\"2026-01-0{}T10:00:00Z\" w:initials=\"X\">{ps}</w:comment>",
        (id % 9) + 1
    )
}

/// A `w15:commentEx` row.
pub(crate) fn ex(para: &str, parent: Option<&str>, done: bool) -> String {
    let parent = parent.map_or_else(String::new, |p| format!(" w15:paraIdParent=\"{p}\""));
    format!(
        "<w15:commentEx w15:paraId=\"{para}\"{parent} w15:done=\"{}\"/>",
        u8::from(done)
    )
}

/// A `w16cid:commentId` row.
pub(crate) fn id_row(para: &str, durable: &str) -> String {
    format!("<w16cid:commentId w16cid:paraId=\"{para}\" w16cid:durableId=\"{durable}\"/>")
}

/// A `w16cex:commentExtensible` row.
pub(crate) fn ext_row(durable: &str) -> String {
    format!("<w16cex:commentExtensible w16cex:durableId=\"{durable}\"/>")
}

/// The range markers and the reference run of one comment around `text`.
pub(crate) fn ranged(id: i64, text: &str) -> String {
    format!(
        "<w:commentRangeStart w:id=\"{id}\"/><w:r><w:t>{text}</w:t></w:r><w:commentRangeEnd w:id=\"{id}\"/><w:r><w:commentReference w:id=\"{id}\"/></w:r>"
    )
}

/// The shape of Word's threaded fixture: a comment and its reply on one range, resolved, and a second
/// comment whose range starts at the end of one paragraph and ends in the next.
pub(crate) fn threaded() -> Pkg {
    let body = "<w:p><w:commentRangeStart w:id=\"0\"/><w:commentRangeStart w:id=\"1\"/><w:r><w:t>target one</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r><w:commentRangeEnd w:id=\"1\"/><w:r><w:commentReference w:id=\"1\"/></w:r><w:r><w:t>.</w:t></w:r><w:commentRangeStart w:id=\"2\"/></w:p><w:p><w:r><w:t>target two</w:t></w:r><w:commentRangeEnd w:id=\"2\"/><w:r><w:commentReference w:id=\"2\"/></w:r></w:p>";
    Pkg::new(body)
        .comments(&[
            comment(0, "Ann", &[Some("317E22E5")], "parent"),
            comment(1, "Bob", &[Some("1541EBE8")], "reply"),
            comment(2, "Ann", &[Some("01A5D985")], "other"),
        ])
        .extended(&[
            ex("317E22E5", None, true),
            ex("1541EBE8", Some("317E22E5"), true),
            ex("01A5D985", None, false),
        ])
        .ids(&[
            id_row("317E22E5", "385D7929"),
            id_row("1541EBE8", "0430989E"),
            id_row("01A5D985", "508E5E3A"),
        ])
        .extensible(&[
            ext_row("385D7929"),
            ext_row("0430989E"),
            ext_row("508E5E3A"),
        ])
        .people(&["Ann", "Bob"])
}
