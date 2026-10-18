//! Finding and creating the comment parts of a document: `comments.xml` when there is none, and
//! `commentsExtended.xml`, which threads and the done flag need. The other extension parts are kept up to
//! date when they exist and are not created (`docs/design/comments-and-notes.md`, section 15).

use crate::api::error::{ErrorCode, StetError};
use crate::package::NewPart;
use crate::revisions::handle::PartWriter;
use crate::schema::PartKind;
use crate::state::PartsView;
use crate::threads::{CommentParts, Threads};
use crate::wordml::StoryKind;
use std::sync::Arc;

const STRICT_W: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const TRANSITIONAL_BASE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_BASE: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// The comment parts of a document by name, and what the main part says about itself.
#[derive(Clone, Debug)]
pub(super) struct Names {
    /// The main document part.
    pub(super) main: Arc<str>,
    /// True if the main part is in the Strict namespace, so that a part made here is too.
    pub(super) strict: bool,
    /// `comments.xml`.
    pub(super) comments: Option<Arc<str>>,
    /// `commentsExtended.xml`.
    pub(super) extended: Option<Arc<str>>,
    /// `commentsIds.xml`.
    pub(super) ids: Option<Arc<str>>,
    /// `commentsExtensible.xml`.
    pub(super) extensible: Option<Arc<str>>,
    /// `people.xml`.
    pub(super) people: Option<Arc<str>>,
}

/// The parts, as a view sees them.
pub(super) fn names(view: &PartsView<'_>) -> Result<Names, StetError> {
    let list = view.story_specs()?;
    let main = list
        .0
        .iter()
        .find(|s| s.kind == StoryKind::Body)
        .and_then(|s| view.id(&s.part).map(|id| (id, Arc::clone(&s.part))))
        .ok_or_else(|| StetError::new(ErrorCode::Internal, "the document has no main part"))?;
    let parts = CommentParts::find(view, main.0)?;
    let name = |p: Option<crate::state::PartId>| p.and_then(|p| view.name(p)).cloned();
    Ok(Names {
        strict: view.tree(main.0)?.root().namespace() == STRICT_W,
        main: main.1,
        comments: name(parts.comments),
        extended: name(parts.extended),
        ids: name(parts.ids),
        extensible: name(parts.extensible),
        people: name(parts.people),
    })
}

fn content_type(kind: PartKind) -> &'static str {
    match kind {
        PartKind::Comments => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"
        }
        _ => "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml",
    }
}

/// The relationship type of a part we create, in the spelling of the document: a Strict file names the
/// comments part with the `purl.oclc.org` base, and the Microsoft extension types have one spelling.
fn rel_type(kind: PartKind, strict: bool) -> String {
    let transitional = kind.relationship_type();
    match (strict, transitional.strip_prefix(TRANSITIONAL_BASE)) {
        (true, Some(rest)) => format!("{STRICT_BASE}{rest}"),
        _ => transitional.to_owned(),
    }
}

/// A part name that no part has: the usual one, or the usual one with a number.
fn free_name(view: &PartsView<'_>, usual: &str) -> String {
    if !view.contains(usual) {
        return usual.to_owned();
    }
    let (stem, ext) = usual.rsplit_once('.').unwrap_or((usual, "xml"));
    (2..)
        .map(|n| format!("{stem}{n}.{ext}"))
        .find(|n| !view.contains(n))
        .unwrap_or_else(|| usual.to_owned())
}

/// Adds a part that the main part relates to.
fn add(
    tx: &mut PartWriter<'_, '_>,
    n: &Names,
    kind: PartKind,
    xml: String,
) -> Result<Arc<str>, StetError> {
    let name = free_name(&tx.parts(), kind.default_path());
    let rel = rel_type(kind, n.strict);
    tx.add_part(NewPart {
        name: &name,
        content_type: content_type(kind),
        bytes: xml.into_bytes(),
        related_from: Some((&n.main, &rel)),
    })?;
    let view = tx.parts();
    view.id(&name)
        .and_then(|id| view.name(id))
        .cloned()
        .ok_or_else(|| StetError::new(ErrorCode::Internal, "a part that was added is not there"))
}

/// `comments.xml`: the part that exists, or a new empty one.
pub(super) fn ensure_comments(tx: &mut PartWriter<'_, '_>) -> Result<Arc<str>, StetError> {
    let n = names(&tx.parts())?;
    if let Some(c) = n.comments {
        return Ok(c);
    }
    let w = if n.strict {
        STRICT_W
    } else {
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    };
    let w14 = crate::threads::NS_W14;
    let xml = format!(
        "{DECL}<w:comments xmlns:w=\"{w}\" xmlns:mc=\"{MC}\" xmlns:w14=\"{w14}\" mc:Ignorable=\"w14\"></w:comments>"
    );
    add(tx, &n, PartKind::Comments, xml)
}

/// `commentsExtended.xml`: the part that exists, or a new one that has a row, open and with no parent,
/// for every comment whose last paragraph has a `paraId`.
pub(super) fn ensure_extended(tx: &mut PartWriter<'_, '_>) -> Result<Arc<str>, StetError> {
    let n = names(&tx.parts())?;
    if let Some(e) = n.extended {
        return Ok(e);
    }
    let w15 = crate::threads::NS_W15;
    let rows: String = Threads::of(&tx.parts())?
        .comments()
        .iter()
        .filter_map(|c| c.para_id)
        .map(|p| format!("<w15:commentEx w15:paraId=\"{p:08X}\" w15:done=\"0\"/>"))
        .collect();
    let xml = format!(
        "{DECL}<w15:commentsEx xmlns:w15=\"{w15}\" xmlns:mc=\"{MC}\" mc:Ignorable=\"w15\">{rows}</w15:commentsEx>"
    );
    add(tx, &n, PartKind::CommentsExtended, xml)
}
