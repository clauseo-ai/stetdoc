//! Deleting a note: its references, tracked as deletions, and its paragraphs.
//!
//! This is the shape Word writes (`RP050-Deleted-Footnote`): the reference run inside a `w:del` in the
//! text, and in the note every paragraph's mark and runs deleted, the last paragraph's mark too, though no
//! paragraph follows it. Accepting removes the note (the compiler's resolver does it when the last
//! reference goes), and rejecting restores it. An untracked deletion takes the references out and then the
//! note, through the same cleanup. Inserting a note is not built in this release.

use super::errors::{internal, unsupported};
use super::{CommentWarning, errors};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::revisions::compiler::Compiler;
use crate::revisions::notes::RemovedNote;
use crate::state::PartsView;
use crate::wordml::{BlockKey, BlockKind, NodeId, StoryKind, UnitKind};
use std::sync::Arc;

/// What deleting a note did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoteDeleted {
    /// True for an endnote.
    pub(crate) endnote: bool,
    /// The note's `w:id`.
    pub(crate) id: i64,
    /// The paragraphs that held a reference, and the note's own blocks, for the receipt and the scope guard.
    pub(crate) blocks: Vec<BlockKey>,
    /// The parts written: the stories that held a reference, and the notes part.
    pub(crate) parts: Vec<Arc<str>>,
    /// The notes that an untracked deletion removed outright.
    pub(crate) removed: Vec<RemovedNote>,
    /// Warnings.
    pub(crate) warnings: Vec<CommentWarning>,
}

/// A reference to the note, found in the text.
struct Reference {
    block: BlockKey,
    at: usize,
}

/// What is known of the note before anything is written.
struct Found {
    notes_part: Arc<str>,
    blocks: Vec<(BlockKey, BlockKind)>,
    references: Vec<Reference>,
    warnings: Vec<CommentWarning>,
}

/// The enclosing paragraph of an element, if it has one.
fn paragraph_of(tree: &crate::xml::SpanTree, node: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(node).parent();
    while let Some(n) = cur {
        if n.local_name() == "p" {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

fn find(view: &PartsView<'_>, endnote: bool, id: i64) -> Result<Found, StetError> {
    let kind = if endnote {
        StoryKind::Endnotes
    } else {
        StoryKind::Footnotes
    };
    let word = if endnote { "endnote" } else { "footnote" };
    let list = view.story_specs()?;
    let spec = list
        .0
        .iter()
        .find(|s| s.kind == kind)
        .ok_or_else(|| errors::not_found_note(word, id))?;
    let part = view
        .id(&spec.part)
        .ok_or_else(|| internal("a story has no part number"))?;
    let story = view.story(part)?;
    let wanted = if endnote {
        UnitKind::Endnote
    } else {
        UnitKind::Footnote
    };
    let unit = story
        .units()
        .iter()
        .position(|u| u.kind == wanted && u.id == Some(id))
        .ok_or_else(|| errors::not_found_note(word, id))?;
    let blocks = story
        .blocks()
        .iter()
        .filter(|e| e.unit as usize == unit && e.parent.is_none())
        .map(|e| (story.key(e), e.kind))
        .collect();
    let mut references = Vec::new();
    let mut warnings = Vec::new();
    let tag = if endnote {
        "endnoteReference"
    } else {
        "footnoteReference"
    };
    for other in list.0.iter().filter(|s| s.kind != kind) {
        let Some(pid) = view.id(&other.part) else {
            continue;
        };
        let (st, tree) = (view.story(pid)?, view.tree(pid)?);
        let mut stack = vec![tree.root()];
        let mut here = Vec::new();
        while let Some(n) = stack.pop() {
            if n.local_name() == tag
                && n.attr(n.namespace(), "id")
                    .is_some_and(|v| v == id.to_string())
            {
                here.push(n.id());
            }
            let kids: Vec<_> = n.child_elements().collect();
            stack.extend(kids.into_iter().rev());
        }
        for node in here {
            let Some(p) = paragraph_of(tree, node) else {
                continue;
            };
            let Some(index) = st.index_of(p) else {
                warnings.push(CommentWarning {
                    code: "reference_in_text_box",
                    message: format!("A reference to the {word} is inside a text box, which cannot be edited, so it was left."),
                });
                continue;
            };
            let Some(paragraph) = st.paragraph(tree, index) else {
                continue;
            };
            let map = paragraph.map();
            let at = map
                .offset_of(Mode::Final, node, 0)
                .filter(|at| map.text(Mode::Final).chars().nth(*at) == Some('\u{2}'));
            // A reference that is not in the final text is already deleted.
            if let Some(at) = at {
                references.push(Reference {
                    block: st.key(
                        st.blocks()
                            .get(index)
                            .ok_or_else(|| internal("a block index is out of range"))?,
                    ),
                    at,
                });
            }
        }
    }
    Ok(Found {
        notes_part: Arc::clone(&spec.part),
        blocks,
        references,
        warnings,
    })
}

/// Deletes the footnote or endnote `id`. Whether the compiler writes revisions is the compiler's own: tracked,
/// each reference run is deleted as text is, and each paragraph of the note with its mark, the last one
/// too. Untracked, the references are taken out and the note goes with them. An untracked deletion of a
/// note that nothing refers to is `unsupported`, since taking its paragraphs out would leave a note with
/// no block, which the schema forbids.
pub(crate) fn delete_note(
    c: &mut Compiler<'_, '_>,
    endnote: bool,
    id: i64,
) -> Result<NoteDeleted, StetError> {
    let tracked = c.tracked();
    let found = find(&c.tx().parts(), endnote, id)?;
    if !tracked && found.references.is_empty() {
        return Err(unsupported(
            "Nothing refers to this note, and an untracked deletion would leave a note with no paragraph. Delete it tracked, or leave it.",
        ));
    }
    let mut blocks: Vec<BlockKey> = Vec::new();
    let mut parts: Vec<Arc<str>> = Vec::new();
    // The last reference first, so that the offsets of the earlier ones in a paragraph still hold.
    for r in found.references.iter().rev() {
        c.replace_text(&r.block, r.at..r.at + 1, "")?;
        if !blocks.contains(&r.block) {
            blocks.push(r.block.clone());
        }
        if !parts.contains(r.block.part()) {
            parts.push(Arc::clone(r.block.part()));
        }
    }
    let mut removed = Vec::new();
    if tracked {
        for (key, kind) in &found.blocks {
            match kind {
                BlockKind::Paragraph => c.delete_block_with_mark(key)?,
                _ => c.delete_block(key)?,
            };
            blocks.push(key.clone());
        }
    } else {
        removed = c.remove_unreferenced_notes(false)?;
        blocks.extend(found.blocks.iter().map(|(k, _)| k.clone()));
    }
    parts.push(found.notes_part);
    Ok(NoteDeleted {
        endnote,
        id,
        blocks,
        parts,
        removed,
        warnings: found.warnings,
    })
}
