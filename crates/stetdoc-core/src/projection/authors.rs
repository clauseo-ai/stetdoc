//! The table of revision authors and their short codes, for `accept` and `reject` by author and for the filter of
//! `Document::revisions`: the codes the views show, so that `JD` names the same person everywhere. Also the ordinals of the
//! notes, which `delete-note` names as the view shows them (`fn3`).

use super::model_inline::NoteKind;
use crate::api::error::StetError;
use crate::state::PartsView;

/// `(code, name)` for every author of a revision or a comment, in order of first appearance. A revision with no author is by
/// `Unknown`.
pub(crate) fn authors(parts: PartsView<'_>) -> Result<Vec<(String, String)>, StetError> {
    let facts = parts.doc_facts()?;
    Ok(facts
        .authors
        .iter()
        .map(|a| (a.code.to_string(), a.name.to_string()))
        .collect())
}

/// `(endnote, ordinal, w:id)` for every note the body refers to: the number the `Redline` view shows after `fn` or `en`,
/// counted per kind from the first reference in the body, every reference counted, and the note it names: the same id in
/// every view. The `Final` and `Original` views add the number Word shows as the mark once their changes are resolved
/// (`DocFacts::ordinal_in`).
pub(crate) fn note_ids(parts: PartsView<'_>) -> Result<Vec<(bool, u32, i64)>, StetError> {
    let facts = parts.doc_facts()?;
    let mut out: Vec<(bool, u32, i64)> = facts
        .ordinals
        .iter()
        .map(|((kind, id), n)| (*kind == NoteKind::Endnote, *n, *id))
        .collect();
    out.sort_unstable();
    Ok(out)
}
