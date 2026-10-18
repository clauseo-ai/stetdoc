//! Which parts are stories: `stories`, and the plain relationship data `state` passes in.
//!
//! A story is a WordprocessingML idea, so the knowledge of which relationship types make stories
//! lives here and not in `state`. `state` converts the package's relationships into [`Rels`] and never
//! inspects a type. See `docs/design/wordml.md` 3.1.

use super::names::{is_w, r_attr};
use super::story::{SECTION_BREAK, Story, StoryKind};
use crate::schema::PartKind;
use crate::xml::SpanTree;
use std::sync::Arc;

/// Where a relationship points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RelTarget {
    /// A part of the package, named as the package entry spells it.
    Part(String),
    /// A target outside the package.
    External(String),
    /// A target that names no part of the package.
    Invalid,
}

/// One relationship of a part, as plain data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rel {
    /// The relationship id, such as `rId7`.
    pub id: String,
    /// The relationship type URI.
    pub rel_type: String,
    /// What it points at.
    pub target: RelTarget,
}

/// The relationships of one part, in the order the relationships part lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rels(Vec<Rel>);

impl Rels {
    /// Wraps relationships in the order they were listed.
    pub fn new(rels: Vec<Rel>) -> Rels {
        Rels(rels)
    }

    /// All relationships, in order.
    pub fn iter(&self) -> impl Iterator<Item = &Rel> {
        self.0.iter()
    }

    /// The relationship with this id. If an id is repeated, the first.
    pub fn get(&self, id: &str) -> Option<&Rel> {
        self.0.iter().find(|r| r.id == id)
    }

    /// The part the first relationship of this kind points at, if it points at a part.
    pub(super) fn first_part(&self, kind: RelKind) -> Option<&str> {
        self.0
            .iter()
            .filter(|r| kind.matches(&r.rel_type))
            .find_map(|r| match &r.target {
                RelTarget::Part(name) => Some(name.as_str()),
                _ => None,
            })
    }
}

/// A story part, and what kind of story it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StorySpec {
    /// The part name, as `state` spells it.
    pub part: Arc<str>,
    /// What kind of story it is.
    pub kind: StoryKind,
}

/// The relationship types `wordml` recognizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RelKind {
    Header,
    Footer,
    Footnotes,
    Endnotes,
    Comments,
    CommentsExtended,
    CommentsIds,
    CommentsExtensible,
    Styles,
    Numbering,
}

const TRANSITIONAL_BASE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_BASE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

impl RelKind {
    /// The Transitional relationship type URI, from `schema`, which holds the one table of them.
    fn transitional(self) -> &'static str {
        let kind = match self {
            RelKind::Header => PartKind::Header,
            RelKind::Footer => PartKind::Footer,
            RelKind::Footnotes => PartKind::Footnotes,
            RelKind::Endnotes => PartKind::Endnotes,
            RelKind::Comments => PartKind::Comments,
            RelKind::CommentsExtended => PartKind::CommentsExtended,
            RelKind::CommentsIds => PartKind::CommentsIds,
            RelKind::CommentsExtensible => PartKind::CommentsExtensible,
            RelKind::Styles => PartKind::Styles,
            RelKind::Numbering => PartKind::Numbering,
        };
        kind.relationship_type()
    }

    /// True if `rel_type` is this type in the Transitional or the Strict spelling.
    pub(super) fn matches(self, rel_type: &str) -> bool {
        let transitional = self.transitional();
        if rel_type == transitional {
            return true;
        }
        match (
            rel_type.strip_prefix(STRICT_BASE),
            transitional.strip_prefix(TRANSITIONAL_BASE),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

fn add(out: &mut Vec<StorySpec>, part: &str, kind: StoryKind) {
    if !out.iter().any(|s| &*s.part == part) {
        out.push(StorySpec {
            part: Arc::from(part),
            kind,
        });
    }
}

/// The stories of a document, in a fixed order with each part once: the body, then headers and
/// footers in the order the `w:sectPr` elements name them, then footnotes, endnotes and comments.
///
/// A header or footer reference counts only when its `r:id` names a relationship of the matching
/// type that points at a part. A part that no `sectPr` names is not a story. This scans the body to
/// find the section breaks; a caller that has the body's [`Story`] already uses [`stories_of`].
pub fn stories(main_part: &str, main: &SpanTree, rels: &Rels) -> Vec<StorySpec> {
    let main_name: Arc<str> = Arc::from(main_part);
    let body = Story::scan(&main_name, StoryKind::Body, main).ok();
    collect(main_part, body.as_ref(), main, rels)
}

/// The stories of a document, as [`stories`], from the body's story and the tree it was scanned
/// from, so that the body is not scanned a second time.
pub fn stories_of(body: &Story, main: &SpanTree, rels: &Rels) -> Vec<StorySpec> {
    collect(body.part(), Some(body), main, rels)
}

fn collect(main_part: &str, body: Option<&Story>, main: &SpanTree, rels: &Rels) -> Vec<StorySpec> {
    let mut out = Vec::new();
    add(&mut out, main_part, StoryKind::Body);
    if let Some(body) = body {
        let mut sections = Vec::new();
        for e in body.blocks() {
            if e.has(SECTION_BREAK)
                && let Some(sect) = main
                    .node_ref(e.node)
                    .child_elements()
                    .next()
                    .and_then(|ppr| ppr.child_elements().find(|c| is_w(c, "sectPr")))
            {
                sections.push(sect);
            }
        }
        let final_sect = main
            .root()
            .child_elements()
            .find(|c| is_w(c, "body"))
            .and_then(|b| b.child_elements().filter(|c| is_w(c, "sectPr")).last());
        sections.extend(final_sect);
        for sect in sections {
            for reference in sect.child_elements() {
                let (kind, story) = if is_w(&reference, "headerReference") {
                    (RelKind::Header, StoryKind::Header)
                } else if is_w(&reference, "footerReference") {
                    (RelKind::Footer, StoryKind::Footer)
                } else {
                    continue;
                };
                let Some(id) = r_attr(&reference, "id") else {
                    continue;
                };
                let Some(rel) = rels.get(&id) else { continue };
                if let (true, RelTarget::Part(name)) = (kind.matches(&rel.rel_type), &rel.target) {
                    add(&mut out, name, story);
                }
            }
        }
    }
    for (kind, story) in [
        (RelKind::Footnotes, StoryKind::Footnotes),
        (RelKind::Endnotes, StoryKind::Endnotes),
        (RelKind::Comments, StoryKind::Comments),
    ] {
        if let Some(name) = rels.first_part(kind) {
            add(&mut out, name, story);
        }
    }
    out
}
