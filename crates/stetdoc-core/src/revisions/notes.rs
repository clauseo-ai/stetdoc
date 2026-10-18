//! Notes whose last reference is gone (design `revisions.md`, 3.6.4, and step 5 of the resolver). A deleted
//! footnote is its reference run in a `w:del` in the body and its paragraphs deleted in `footnotes.xml`
//! (`RP050`). Accepting leaves a note with no content, which the schema forbids, so a note that no
//! reference names is removed. The function is the resolver's, and it is what the operations on notes
//! call after they take a reference out untracked.

use super::cleanup::is_attached;
use super::names::{is_w, w_attr};
use super::tree_ops::detach;
use crate::api::error::StetError;
use crate::state::{PartsView, Retired, Tx};
use crate::wordml::{BlockKey, NodeId, StoryKind, UnitKind};
use crate::xml::SpanTree;
use std::collections::HashSet;
use std::sync::Arc;

/// A note that was removed because nothing refers to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemovedNote {
    /// The part: `word/footnotes.xml` or `word/endnotes.xml`.
    pub part: Arc<str>,
    /// The `w:id` of the note.
    pub id: i64,
    /// True for an endnote.
    pub endnote: bool,
}

/// True if the subtree at `node` holds a footnote or an endnote reference.
pub(super) fn holds_note_reference(tree: &SpanTree, node: NodeId) -> bool {
    let mut stack = vec![tree.node_ref(node)];
    while let Some(n) = stack.pop() {
        if is_w(&n, "footnoteReference") || is_w(&n, "endnoteReference") {
            return true;
        }
        stack.extend(n.child_elements());
    }
    false
}

/// The ids that the references in the stories of a transaction name.
#[derive(Clone, Debug, Default)]
pub(super) struct Referenced {
    footnotes: HashSet<i64>,
    endnotes: HashSet<i64>,
}

impl Referenced {
    fn has(&self, endnote: bool, id: i64) -> bool {
        if endnote {
            self.endnotes.contains(&id)
        } else {
            self.footnotes.contains(&id)
        }
    }
}

/// The ids that the references in every story of the transaction name.
pub(super) fn referenced_in_stories(tx: &Tx<'_>) -> Result<Referenced, StetError> {
    referenced_in(&tx.parts())
}

/// The ids that the references in every story of a view name.
fn referenced_in(view: &PartsView<'_>) -> Result<Referenced, StetError> {
    let mut out = Referenced::default();
    for spec in view.story_specs()?.0.iter() {
        let Some(id) = view.id(&spec.part) else {
            continue;
        };
        let mut stack = vec![view.tree(id)?.root()];
        while let Some(n) = stack.pop() {
            if is_w(&n, "footnoteReference") {
                out.footnotes.extend(super::names::rev_id(&n));
            } else if is_w(&n, "endnoteReference") {
                out.endnotes.extend(super::names::rev_id(&n));
            }
            stack.extend(n.child_elements());
        }
    }
    Ok(out)
}

/// A note that is not an ordinary one: the separators that Word keeps in the part.
fn is_special(tree: &SpanTree, note: NodeId) -> bool {
    w_attr(&tree.node_ref(note), "type").is_some_and(|t| t != "normal")
}

/// Removes the footnotes and endnotes that lost their last reference in this transaction, and returns them:
/// the ones that a reference named when the transaction began and none names now. A note that nothing
/// named before is not this transaction's to remove. The blocks that were in them leave the tree; with
/// `rejected`, the ones that had a ref are retired as rejected, as a rejected insertion does, and otherwise
/// the audit records them as deleted. This is the entry point for an operation that has just taken a
/// reference out. The resolver uses the same rule step by step, from what the references named before the
/// step.
pub(crate) fn remove_unreferenced_notes(
    tx: &mut Tx<'_>,
    rejected: bool,
) -> Result<Vec<RemovedNote>, StetError> {
    let at_begin = referenced_in(&tx.base().parts())?;
    remove_notes(tx, rejected, &at_begin)
}

/// Removes the footnotes and endnotes that were named in `before`, and that no reference names now.
pub(super) fn remove_notes_that_lost_their_references(
    tx: &mut Tx<'_>,
    rejected: bool,
    before: &Referenced,
) -> Result<Vec<RemovedNote>, StetError> {
    remove_notes(tx, rejected, before)
}

fn remove_notes(
    tx: &mut Tx<'_>,
    rejected: bool,
    before: &Referenced,
) -> Result<Vec<RemovedNote>, StetError> {
    let now = referenced_in_stories(tx)?;
    let specs = tx.parts().story_specs()?;
    let mut removed = Vec::new();
    for spec in specs.0.iter() {
        let is_end = match spec.kind {
            StoryKind::Footnotes => false,
            StoryKind::Endnotes => true,
            _ => continue,
        };
        let (doomed, retire): (Vec<(NodeId, i64)>, Vec<BlockKey>) = {
            let view = tx.parts();
            let Some(id) = view.id(&spec.part) else {
                continue;
            };
            let story = view.story(id)?;
            let tree = view.tree(id)?;
            let mut doomed: Vec<(NodeId, i64)> = Vec::new();
            let mut retire: Vec<BlockKey> = Vec::new();
            for (k, unit) in story.units().iter().enumerate() {
                let wanted = matches!(
                    (unit.kind, is_end),
                    (UnitKind::Footnote, false) | (UnitKind::Endnote, true)
                );
                let Some(note_id) = unit.id.filter(|_| wanted) else {
                    continue;
                };
                if now.has(is_end, note_id)
                    || !before.has(is_end, note_id)
                    || is_special(tree, unit.node)
                    || !is_attached(tree, unit.node)
                {
                    continue;
                }
                doomed.push((unit.node, note_id));
                if rejected {
                    let held = story
                        .blocks()
                        .iter()
                        .filter(|e| usize::try_from(e.unit).ok() == Some(k))
                        .map(|e| story.key(e));
                    retire.extend(held.filter(|key| view.ref_of(key).is_some()));
                }
            }
            (doomed, retire)
        };
        if doomed.is_empty() {
            continue;
        }
        let tree = tx.part_mut(&spec.part)?;
        for (node, _) in &doomed {
            detach(tree, *node)?;
        }
        for key in retire {
            tx.retire(&key, Retired::Rejected)?;
        }
        removed.extend(doomed.into_iter().map(|(_, id)| RemovedNote {
            part: spec.part.clone(),
            id,
            endnote: is_end,
        }));
    }
    Ok(removed)
}
