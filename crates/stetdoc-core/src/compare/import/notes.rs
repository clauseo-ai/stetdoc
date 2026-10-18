//! Notes of imported content (design `compare.md`, 7.5).
//!
//! A note reference in content that is copied into the revised document needs its note there too. The note body is copied
//! into the revised notes part under a new id, as a `w:footnote` (or `w:endnote`) whose paragraphs the build marks as
//! deleted, so that a reference and its note are always written together, and accepting the deletion of the reference removes
//! the note once nothing names it.

use super::{Copied, Cx, Importer};
use crate::api::error::StetError;
use crate::revisions::import_writer::ImportWriter;
use crate::wordml::NodeId;
use std::collections::HashMap;
use std::sync::Arc;

/// A note that import copied for the revised document's notes part, which the build adds with `add_deleted_note` once its
/// reference is in the story.
#[derive(Clone, Debug)]
pub(in crate::compare) struct NewNote {
    /// The revised notes part.
    pub part: Arc<str>,
    /// True for an endnote.
    pub endnote: bool,
    /// Its id there.
    pub id: i64,
    /// The paragraphs of the note, detached copies.
    pub paragraphs: Vec<Copied>,
    /// The part that holds the copies with a reference to it.
    pub holder_part: Option<Arc<str>>,
    /// The top nodes of those copies: the note is added only if one of them is in the story, so that no note is left
    /// without a reference when a copy is abandoned (the fallback of a paragraph replaced whole).
    pub holders: Vec<NodeId>,
}

/// What the importer remembers about notes.
#[derive(Default)]
pub(super) struct NoteState {
    /// (is an endnote, original id) to the id in the revised document, `None` if it could not be carried.
    pub(super) map: HashMap<(bool, i64), Option<i64>>,
    /// The notes copied since the build last asked.
    pub(super) fresh: Vec<NewNote>,
    /// The notes that references named during the copies in progress, innermost last: each copy takes its own when it is
    /// written and records itself as a holder of them.
    pub(super) hits: Vec<(bool, i64)>,
    /// The next id in each revised notes part.
    next: HashMap<String, i64>,
}

impl NoteState {
    /// Forgets the note that was given `new` in the revised document.
    pub(super) fn forget(&mut self, endnote: bool, new: i64) {
        self.map.retain(|k, v| !(k.0 == endnote && *v == Some(new)));
    }
}

fn notes_parts(
    view: &crate::state::PartsView<'_>,
    endnote: bool,
) -> Result<Option<Arc<str>>, StetError> {
    let specs = view.story_specs()?;
    let want = if endnote {
        crate::wordml::StoryKind::Endnotes
    } else {
        crate::wordml::StoryKind::Footnotes
    };
    Ok(specs
        .0
        .iter()
        .find(|s| s.kind == want)
        .map(|s| Arc::clone(&s.part)))
}

/// The note `id` of the original as a note of the revised document: its id there.
pub(super) fn import(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    endnote: bool,
    id: i64,
) -> Result<Option<i64>, StetError> {
    if let Some(done) = imp.notes.map.get(&(endnote, id)) {
        if done.is_some() {
            imp.notes.hits.push((endnote, id));
        }
        return Ok(*done);
    }
    imp.notes.map.insert((endnote, id), None);
    let src = notes_parts(&imp.orig.parts(), endnote)?;
    let dst = notes_parts(&tx.parts(), endnote)?;
    let (Some(src), dst) = (src, dst) else {
        return Ok(None);
    };
    let dst = match dst {
        Some(d) => d,
        None => match add_notes_part(imp, tx, endnote, &src)? {
            Some(d) => d,
            None => return Ok(None),
        },
    };
    // the paragraphs of the original note
    let paras = {
        let view = imp.orig.parts();
        let Some(pid) = view.id(&src) else {
            return Ok(None);
        };
        let tree = view.tree(pid)?;
        let local = if endnote { "endnote" } else { "footnote" };
        let Some(note) = tree.root().child_elements().find(|c| {
            c.local_name() == local
                && c.attrs()
                    .any(|a| a.local_name() == "id" && a.value().parse::<i64>().ok() == Some(id))
        }) else {
            return Ok(None);
        };
        let paras: Vec<NodeId> = note
            .child_elements()
            .filter(|c| matches!(c.local_name(), "p" | "tbl"))
            .map(|c| c.id())
            .collect();
        paras
    };
    let new_id = {
        let next = match imp.notes.next.get(dst.as_ref()) {
            Some(n) => *n,
            None => {
                let view = tx.parts();
                let Some(pid) = view.id(&dst) else {
                    return Ok(None);
                };
                let tree = view.tree(pid)?;
                tree.root()
                    .child_elements()
                    .filter_map(|c| {
                        c.attrs()
                            .find(|a| a.local_name() == "id")
                            .and_then(|a| a.value().parse::<i64>().ok())
                    })
                    .max()
                    .unwrap_or(0)
                    + 1
            }
        };
        imp.notes.next.insert(dst.to_string(), next + 1);
        next
    };
    let cx = Cx {
        src: &src,
        dst: &dst,
    };
    let mut copies: Vec<Copied> = Vec::new();
    for p in paras {
        copies.push(imp.block(tx, cx, p)?);
    }
    imp.notes.fresh.push(NewNote {
        part: Arc::clone(&dst),
        endnote,
        id: new_id,
        paragraphs: copies,
        holder_part: None,
        holders: Vec::new(),
    });
    imp.notes.map.insert((endnote, id), Some(new_id));
    imp.notes.hits.push((endnote, id));
    Ok(Some(new_id))
}

/// The revised document has no part for this kind of note: one is added, with the separator notes of the original's, so that a note of
/// deleted content has somewhere to go.
fn add_notes_part(
    imp: &mut Importer<'_>,
    tx: &mut ImportWriter<'_, '_>,
    endnote: bool,
    src: &Arc<str>,
) -> Result<Option<Arc<str>>, StetError> {
    let view = imp.orig.parts();
    let (Some(bytes), Some(content_type)) = (view.part_bytes(src)?, view.part_content_type(src)?)
    else {
        return Ok(None);
    };
    // the relationship type the original's main part uses for these notes
    let suffix = if endnote { "/endnotes" } else { "/footnotes" };
    let main_src = view.story_specs()?.0.first().map(|s| Arc::clone(&s.part));
    let rel_type = main_src
        .and_then(|m| view.id(&m))
        .map(|id| view.rels(id))
        .transpose()?
        .and_then(|rels| {
            rels.iter()
                .find(|r| r.rel_type.ends_with(suffix))
                .map(|r| r.rel_type.clone())
        });
    // a package whose main part does not say the type is given the transitional one, so that the note is not dropped
    let rel_type = rel_type.unwrap_or_else(|| {
        format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships{suffix}")
    });
    let there = tx.parts();
    let Some(main_dst) = there.story_specs()?.0.first().map(|s| Arc::clone(&s.part)) else {
        return Ok(None);
    };
    let name = if there.contains(src) {
        format!(
            "word/{}_o.xml",
            if endnote { "endnotes" } else { "footnotes" }
        )
    } else {
        src.to_string()
    };
    let name = tx.add_notes_part(
        crate::package::NewPart {
            name: &name,
            content_type: &content_type,
            bytes: bytes.as_ref().to_vec(),
            related_from: Some((&main_dst, &rel_type)),
        },
        src,
    )?;
    Ok(Some(name))
}
