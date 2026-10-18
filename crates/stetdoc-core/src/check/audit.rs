//! The comment audit of `threads`, folded in by key (`docs/design/check.md` 4).
//!
//! `threads::audit` says what is wrong with the comment parts and the markers in the stories, with
//! a stable code. `check` runs it on the base and on the transaction, and a finding whose key was
//! not there before is new: a new error blocks like any other violation, a new note goes to
//! `Verified::notes`, and what was already there is carried. The key is the code, the comment the
//! finding is about and the part, and not the message, whose numbers move when rows are added.
//!
//! Each side is cached on the generations of the parts the audit reads, so the audit of one batch is
//! the audit that the next batch starts from.

use super::document::{SchemaPart, is_story, listing};
use super::found::Found;
use super::keys::Key;
use super::verdict::Verdict;
use crate::api::check::{Class, FindingCode};
use crate::api::error::{ErrorCode, StetError};
use crate::schema::PartKind;
use crate::state::{Dep, PartsView};
use crate::threads;
use std::collections::HashSet;
use std::sync::Arc;

/// The variant of the cache that holds an audit.
const AUDIT: u64 = 2;

/// One finding of the audit, as `check` holds it.
pub(super) struct Item {
    pub(super) key: Key,
    pub(super) code: FindingCode,
    pub(super) class: Class,
    /// The part it is about, as the package names it.
    pub(super) part: Option<String>,
    /// Whether it is about a comment, which lives in the comments part.
    about_a_comment: bool,
    pub(super) message: String,
}

/// What the audit said about one state of the document.
pub(super) struct Audited(pub(super) Vec<Item>);

fn internal(message: String) -> StetError {
    StetError::new(ErrorCode::Internal, format!("internal: {message}"))
}

/// Whether the part is one of the parts that comments live in or are described by.
fn is_comment_part(kind: PartKind) -> bool {
    matches!(
        kind,
        PartKind::Comments
            | PartKind::CommentsExtended
            | PartKind::CommentsIds
            | PartKind::CommentsExtensible
            | PartKind::People
    )
}

/// Whether a document has a comments part.
pub(super) fn has_comments(parts: &[SchemaPart]) -> bool {
    parts.iter().any(|p| p.kind == PartKind::Comments)
}

/// Whether a part can change what the audit says.
pub(super) fn is_audited(kind: PartKind) -> bool {
    is_story(kind) || is_comment_part(kind)
}

fn item(finding: &threads::Finding) -> Result<Item, StetError> {
    let Some(code) = FindingCode::from_name(finding.code) else {
        return Err(internal(format!(
            "the comment audit reports {}, which check has no code for",
            finding.code
        )));
    };
    let class = match finding.class {
        threads::Class::Error => Class::Violation,
        threads::Class::Note => Class::Note,
    };
    if code.class() != class {
        return Err(internal(format!(
            "the comment audit gives {} another class than check does",
            finding.code
        )));
    }
    let comment = finding
        .comment
        .map_or_else(|| "-".to_owned(), |id| format!("c{id}"));
    let part = finding.part.as_deref().unwrap_or("-");
    Ok(Item {
        key: Key::rule(code, format!("{comment}|{part}")),
        code,
        class,
        part: finding.part.clone(),
        about_a_comment: finding.comment.is_some(),
        message: finding.message.clone(),
    })
}

/// The audit of the state a view reads, from the cache when none of the parts it reads has changed.
pub(super) fn audit_of(
    view: &PartsView<'_>,
    parts: &[SchemaPart],
) -> Result<Arc<Audited>, StetError> {
    let mut deps = vec![Dep::Package];
    deps.extend(
        parts
            .iter()
            .filter(|p| is_audited(p.kind))
            .map(|p| Dep::Part(p.id)),
    );
    let build = |_: Option<&Audited>| -> Result<Audited, StetError> {
        let items: Result<Vec<Item>, StetError> = threads::audit(view)?.iter().map(item).collect();
        Ok(Audited(items?))
    };
    view.derived(AUDIT, &deps, &build)
}

impl Item {
    /// The finding as the verdict holds it, and the place of its part in the list of parts. A
    /// finding about a comment that names no part is in the comments part.
    pub(super) fn found(&self, parts: &[SchemaPart]) -> (Option<usize>, Found) {
        let named = self
            .part
            .as_deref()
            .and_then(|name| parts.iter().position(|p| *p.name == *name));
        let at = named.or_else(|| {
            self.about_a_comment
                .then(|| parts.iter().position(|p| p.kind == PartKind::Comments))
                .flatten()
        });
        (
            at,
            Found {
                key: self.key.clone(),
                code: self.code,
                node: None,
                message: self.message.clone(),
            },
        )
    }
}

/// The audit of a transaction against the audit of its base. A new error is a violation and a new
/// note is a note. A finding whose key the base had is neither: it is about the comments of the
/// document and not about the blocks the edit changed, so it is not carried into the receipt
/// either, and `Document::check` lists it.
pub(super) fn judge(
    verdict: &mut Verdict,
    (before, after): (&PartsView<'_>, &PartsView<'_>),
    parts: &[SchemaPart],
) -> Result<(), StetError> {
    let old = audit_of(before, &listing(before)?.parts)?;
    let now = audit_of(after, parts)?;
    let was: HashSet<&Key> = old.0.iter().map(|item| &item.key).collect();
    for item in &now.0 {
        if was.contains(&item.key) {
            continue;
        }
        let (at, found) = item.found(parts);
        if item.class == Class::Note {
            verdict.note(at, found);
        } else {
            verdict.add_new(at, found);
        }
    }
    Ok(())
}
