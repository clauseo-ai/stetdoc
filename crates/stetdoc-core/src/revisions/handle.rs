//! A narrow handle on a transaction for the modules that write parts that are not the document's stories.
//!
//! The compiler is the one writer of the document's stories (rule 2 of `ARCHITECTURE.md`): the body, the
//! headers and footers, the footnotes and the endnotes. `comments` writes `comments.xml`, its extension parts,
//! `people.xml` and the style sheet, which hold no revisions. It gets a `PartWriter`, which does what a `Tx`
//! does for those parts only: a part it changes is one the main part relates to with one of their
//! relationship types, and a part it adds is one of them, with that kind's content type and a relationship
//! of that kind from the main part. So no module writes a story, or adds one, past the compiler.

use crate::api::error::{ErrorCode, StetError};
use crate::schema::PartKind;
use crate::state::{NewPart, PartsView, Tx};
use crate::wordml::{IdKind, RelTarget, StoryKind};
use crate::xml::SpanTree;
use std::sync::Arc;

/// The kinds of part a `PartWriter` writes: the comment parts, the people and the style sheet.
const WRITABLE: [PartKind; 6] = [
    PartKind::Comments,
    PartKind::CommentsExtended,
    PartKind::CommentsIds,
    PartKind::CommentsExtensible,
    PartKind::People,
    PartKind::Styles,
];

/// The main part's name: the part of the body story.
fn main_part(view: &PartsView<'_>) -> Result<Arc<str>, StetError> {
    view.story_specs()?
        .0
        .iter()
        .find(|s| s.kind == StoryKind::Body)
        .map(|s| Arc::clone(&s.part))
        .ok_or_else(|| StetError::new(ErrorCode::Internal, "the document has no main part"))
}

/// A transaction as the writer of parts that are not stories sees it.
pub(crate) struct PartWriter<'a, 's> {
    tx: &'a mut Tx<'s>,
}

fn refused(what: &str) -> StetError {
    StetError::new(
        ErrorCode::Internal,
        format!(
            "{what} is not a comment part, the people part or the style sheet, and only the compiler writes the document's stories"
        ),
    )
}

impl<'a, 's> PartWriter<'a, 's> {
    /// A handle over the transaction.
    pub(crate) fn over(tx: &'a mut Tx<'s>) -> PartWriter<'a, 's> {
        PartWriter { tx }
    }

    /// The parts and derived values as the transaction sees them, to read.
    pub(crate) fn parts(&self) -> PartsView<'_> {
        self.tx.parts()
    }

    /// A part's tree, to change: a part that the main part relates to as a comment part, the people part
    /// or the style sheet. Anything else is refused: the body, a header or footer, the footnotes, the
    /// endnotes, any other part and a name that is not a part. The comments part is a story of `wordml` and
    /// is allowed: it holds no revisions and is `comments`' own.
    pub(crate) fn part_mut(&mut self, name: &str) -> Result<&mut SpanTree, StetError> {
        if !self.writable(name)? {
            return Err(refused(name));
        }
        self.tx.part_mut(name)
    }

    /// Whether `name` is a part this handle writes: the target of a relationship from the main part whose
    /// type is one of [`WRITABLE`], and no story but the comments.
    fn writable(&self, name: &str) -> Result<bool, StetError> {
        let view = self.tx.parts();
        let other_story =
            view.story_specs()?.0.iter().any(|s| {
                s.part.eq_ignore_ascii_case(name) && !matches!(s.kind, StoryKind::Comments)
            });
        if other_story {
            return Ok(false);
        }
        let Some(main) = view.id(&main_part(&view)?) else {
            return Ok(false);
        };
        Ok(view.rels(main)?.iter().any(|r| {
            matches!(&r.target, RelTarget::Part(t) if t.eq_ignore_ascii_case(name))
                && PartKind::from_relationship_type(&r.rel_type)
                    .is_some_and(|k| WRITABLE.contains(&k))
        }))
    }

    /// Adds a part, its content type and a relationship to it from the main part. The part is one of
    /// [`WRITABLE`]: the relationship's type and the content type both name that kind. Anything else, a
    /// story among it, is refused, and nothing changes.
    pub(crate) fn add_part(&mut self, new: NewPart<'_>) -> Result<Option<String>, StetError> {
        let main = main_part(&self.tx.parts())?;
        let allowed = match new.related_from {
            Some((source, rel)) if source.eq_ignore_ascii_case(&main) => {
                let kind = PartKind::from_relationship_type(rel);
                kind.is_some_and(|k| WRITABLE.contains(&k))
                    && kind == PartKind::from_content_type(new.content_type)
            }
            _ => false,
        };
        if !allowed {
            return Err(refused(new.name));
        }
        self.tx.add_part(new)
    }

    /// The next annotation id: a comment's `w:id` comes from the counter that revisions use.
    pub(crate) fn next_annotation_id(&mut self) -> Result<u32, StetError> {
        self.tx.next_annotation_id()
    }

    /// A new durable id of a kind, for an element of `part`.
    pub(crate) fn mint_id(&mut self, kind: IdKind, part: &str) -> Result<u32, StetError> {
        self.tx.mint_id(kind, part)
    }
}
