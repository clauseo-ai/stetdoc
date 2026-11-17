//! Footnotes and endnotes that nothing references (`unreferenced_note`). Word keeps such a note in its part and shows it
//! nowhere, so its text is in the package and out of sight. A note counts as referenced when a `w:footnoteReference` or
//! `w:endnoteReference` in any story names its id, a reference inside a tracked deletion included, since rejecting the
//! deletion brings the reference back. The separators (a note with a `w:type` other than `normal`) are not referenced by
//! design and are never reported. The finding is a note and not a violation: a package that already has one can still be
//! edited, and an edit takes a note out with its last reference (`revisions/notes.rs`).

use super::found::Found;
use crate::api::check::FindingCode;
use crate::api::error::StetError;
use crate::schema::is_word_namespace;
use crate::state::PartsView;
use crate::wordml::{StoryKind, UnitKind};
use crate::xml::NodeRef;
use std::collections::HashSet;
use std::sync::Arc;

fn is_w(n: &NodeRef<'_>, local: &str) -> bool {
    n.is_element() && n.local_name() == local && is_word_namespace(n.namespace())
}

fn w_attr(n: &NodeRef<'_>, local: &str) -> Option<String> {
    n.attrs()
        .find(|a| a.local_name() == local && is_word_namespace(a.namespace()))
        .map(|a| a.value().into_owned())
}

fn id_of(n: &NodeRef<'_>) -> Option<i64> {
    w_attr(n, "id").and_then(|v| v.trim().parse::<i64>().ok())
}

/// Every note that no reference names, with the part it is in.
pub(super) fn unreferenced(parts: &PartsView<'_>) -> Result<Vec<(Arc<str>, Found)>, StetError> {
    let specs = parts.story_specs()?;
    let notes: Vec<_> = specs
        .0
        .iter()
        .filter(|s| matches!(s.kind, StoryKind::Footnotes | StoryKind::Endnotes))
        .collect();
    if notes.is_empty() {
        return Ok(Vec::new());
    }
    // (endnote, id) of every reference in every story.
    let mut named: HashSet<(bool, i64)> = HashSet::new();
    for spec in specs.0.iter() {
        let Some(id) = parts.id(&spec.part) else {
            continue;
        };
        let mut stack = vec![parts.tree(id)?.root()];
        while let Some(n) = stack.pop() {
            if is_w(&n, "footnoteReference") {
                named.extend(id_of(&n).map(|i| (false, i)));
            } else if is_w(&n, "endnoteReference") {
                named.extend(id_of(&n).map(|i| (true, i)));
            }
            stack.extend(n.child_elements());
        }
    }
    let mut out = Vec::new();
    for spec in notes {
        let endnote = spec.kind == StoryKind::Endnotes;
        let Some(id) = parts.id(&spec.part) else {
            continue;
        };
        let story = parts.story(id)?;
        let tree = parts.tree(id)?;
        for unit in story.units() {
            if !matches!(unit.kind, UnitKind::Footnote | UnitKind::Endnote) {
                continue;
            }
            let Some(note_id) = unit.id else { continue };
            let node = tree.node_ref(unit.node);
            let special = w_attr(&node, "type").is_some_and(|t| t != "normal");
            if special || named.contains(&(endnote, note_id)) {
                continue;
            }
            let word = if endnote { "endnote" } else { "footnote" };
            let subject = format!("{}#{word} (id {note_id})", spec.part);
            out.push((
                spec.part.clone(),
                Found::rule(FindingCode::UnreferencedNote, &subject, None),
            ));
        }
    }
    Ok(out)
}
