//! A package builder for tests that need styles, numbering, notes, pictures and relationships.

use crate::api::limits::Limits;
use crate::state::State;
use stetdoc_testkit::fixtures::{
    parts::{self, Doc},
    zip::{Entry, Zip},
};

const EXTRA: &str = " xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"";
const W: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"";
const WML: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

/// A package under construction.
#[derive(Default, Clone)]
pub(super) struct Pk {
    /// The body of `word/document.xml`.
    pub body: String,
    /// `styles.xml` content: the `w:style` elements.
    pub styles: Option<String>,
    /// `numbering.xml` content: the abstract numberings and the lists.
    pub numbering: Option<String>,
    /// The footnotes, as `w:footnote` elements.
    pub footnotes: Option<String>,
    /// Other parts: name under `word/`, content type, bytes, and the relationship id to it.
    pub media: Vec<(String, String, Vec<u8>, String)>,
    /// External relationships: id, type suffix, target.
    pub external: Vec<(String, String, String)>,
}

fn rel(id: &str, kind: &str, target: &str, external: bool) -> String {
    let mode = if external {
        " TargetMode=\"External\""
    } else {
        ""
    };
    format!(
        "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\"{mode}/>"
    )
}

impl Pk {
    /// A package with this body.
    pub(super) fn new(body: &str) -> Pk {
        Pk {
            body: body.to_owned(),
            ..Pk::default()
        }
    }

    /// The package's bytes.
    pub(super) fn bytes(&self) -> Vec<u8> {
        let doc = Doc::new().body(&self.body).root_attrs(EXTRA).bytes();
        let mut types = parts::content_types();
        types = types.replace(
            "<Default Extension=\"xml\"",
            "<Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"xml\"",
        );
        let mut rels = String::new();
        let mut entries = Vec::new();
        let mut add = |name: &str, ct: &str, id: &str, kind: &str, xml: String| {
            types = types.replace(
                "</Types>",
                &format!(
                    "<Override PartName=\"/word/{name}\" ContentType=\"{WML}.{ct}+xml\"/></Types>"
                ),
            );
            rels.push_str(&rel(id, kind, name, false));
            entries.push(Entry::new(format!("word/{name}"), xml));
        };
        if let Some(s) = &self.styles {
            add(
                "styles.xml",
                "styles",
                "rId10",
                "styles",
                format!("{}<w:styles {W}>{s}</w:styles>", parts::DECL),
            );
        }
        if let Some(s) = &self.numbering {
            add(
                "numbering.xml",
                "numbering",
                "rId11",
                "numbering",
                format!("{}<w:numbering {W}>{s}</w:numbering>", parts::DECL),
            );
        }
        if let Some(s) = &self.footnotes {
            add(
                "footnotes.xml",
                "footnotes",
                "rId12",
                "footnotes",
                format!("{}<w:footnotes {W}>{s}</w:footnotes>", parts::DECL),
            );
        }
        for (name, ct, bytes, id) in &self.media {
            rels.push_str(&rel(id, "image", name, false));
            if !ct.is_empty() {
                types = types.replace(
                    "</Types>",
                    &format!("<Override PartName=\"/word/{name}\" ContentType=\"{ct}\"/></Types>"),
                );
            }
            entries.push(Entry::new(format!("word/{name}"), bytes.clone()));
        }
        for (id, kind, target) in &self.external {
            rels.push_str(&rel(id, kind, target, true));
        }
        let mut all = parts::entries_with(&doc, &parts::root_rels(), &types);
        all.push(Entry::new(
            "word/_rels/document.xml.rels",
            format!(
                "{}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>",
                parts::DECL
            ),
        ));
        all.extend(entries);
        Zip::new(all).build()
    }

    /// The package, opened.
    pub(super) fn state(&self) -> State {
        State::open(self.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("does not open: {e}"))
    }
}
