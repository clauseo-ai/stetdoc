//! The one place that copies from one package into the other (design `compare.md`, 7).
//!
//! The importer never writes a revision. It produces detached nodes in the redline's trees, and the parts and definitions
//! they need, and the primitives of `revisions` write the revisions around them. Every copy goes through the same steps: read
//! the source subtree as a [`Snip`], rewrite everything in it that names something of the source package (style ids, list ids,
//! relationship ids, note ids, bookmark ids, drawing ids), write it into the target tree with the editor's own calls, and
//! bind the namespaces it uses on the target's root. What cannot be carried (a comment, a hyperlink to an address the target
//! has no relationship for, a drawing whose part has relationships of its own) is dropped and counted, so a comparison says
//! what it left out.

mod canon;
mod fragment;
mod markers;
mod notes;
mod numbering;
mod parts;
mod props;
mod snip;
mod styles;
mod transform;

pub(super) use notes::NewNote;
pub(super) use numbering::ListMatch;
#[cfg(test)]
pub(super) use numbering::list_definitions;

/// A test can make the copy of an original paragraph fail, which no document does: the import refuses nothing a document can
/// hold, and the receipt's account of a block that could not be copied needs one that could not be.
#[cfg(test)]
pub(crate) mod failpoint {
    use std::cell::RefCell;

    thread_local! {
        static TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    /// Makes the copy of every original paragraph whose text holds `text` fail, on this thread, until `disarm`.
    pub(crate) fn arm(text: &str) {
        TEXT.with(|t| *t.borrow_mut() = Some(text.to_string()));
    }

    /// Stops it.
    pub(crate) fn disarm() {
        TEXT.with(|t| *t.borrow_mut() = None);
    }

    /// Whether the copy of a paragraph with this text is to fail.
    pub(super) fn fails(text: &str) -> bool {
        TEXT.with(|t| {
            t.borrow()
                .as_deref()
                .is_some_and(|armed| text.contains(armed))
        })
    }
}

use crate::api::error::{ErrorCode, StetError};
use crate::revisions::import_writer::ImportWriter;
use crate::state::State;
use crate::wordml::{BlockKey, NodeId};
use snip::{Reader, Snip};
use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;

/// What a comparison left out or had to change on the way across, for the receipt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stats {
    /// Comment ranges and references of imported content.
    pub comments_dropped: usize,
    /// Hyperlinks whose address the target has no relationship for: the text stays, the link goes.
    pub hyperlinks_unlinked: usize,
    /// Bookmarks whose name the revised document already has.
    pub bookmarks_dropped: usize,
    /// Drawings, pictures and objects whose parts could not be carried.
    pub objects_dropped: usize,
    /// Note references whose note could not be carried.
    pub notes_dropped: usize,
    /// Lists that the target has no numbering part for.
    pub lists_dropped: usize,
    /// Equations, which a tracked change cannot carry.
    pub math_dropped: usize,
    /// Styles added to the revised document.
    pub styles_imported: usize,
    /// Styles added under a new id because the revised document has another style with that id.
    pub styles_renamed: usize,
    /// Numbering definitions added.
    pub lists_imported: usize,
    /// Parts copied across.
    pub parts_copied: usize,
}

/// The two ends of one copy: the part of the original that is read and the part of the revised document that is written.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cx<'c> {
    /// The original's part.
    pub src: &'c Arc<str>,
    /// The revised document's part.
    pub dst: &'c Arc<str>,
}

/// A block copied into the revised document's tree, detached, with the original block it is a copy of.
#[derive(Clone, Debug)]
pub(super) struct Copied {
    /// The detached node.
    pub node: NodeId,
    /// The block in the original.
    pub source: BlockKey,
}

/// The namespaces and prefixes a read found, which the write needs.
#[derive(Default)]
pub(super) struct Meta {
    used: snip::Used,
    requires: BTreeMap<String, String>,
}

/// The importer of one comparison.
pub(super) struct Importer<'o> {
    orig: &'o State,
    styles: styles::StyleState,
    nums: numbering::NumState,
    rels: parts::RelState,
    notes: notes::NoteState,
    marks: markers::MarkState,
    /// What was left out.
    pub stats: Stats,
}

pub(super) fn internal(message: &str) -> StetError {
    StetError::new(ErrorCode::Internal, message)
}

impl<'o> Importer<'o> {
    /// An importer that reads from `orig`.
    pub(super) fn new(orig: &'o State) -> Importer<'o> {
        Importer {
            orig,
            styles: styles::StyleState::default(),
            nums: numbering::NumState::default(),
            rels: parts::RelState::default(),
            notes: notes::NoteState::default(),
            marks: markers::MarkState::default(),
            stats: Stats::default(),
        }
    }

    /// Copies the block `node` of the original (a paragraph, a row or a table) into the revised document's tree, detached.
    pub(super) fn block(
        &mut self,
        tx: &mut ImportWriter<'_, '_>,
        cx: Cx<'_>,
        node: NodeId,
    ) -> Result<Copied, StetError> {
        let view = self.orig.parts();
        let id = view
            .id(cx.src)
            .ok_or_else(|| internal("the original has no such part"))?;
        #[cfg(test)]
        if crate::wordml::Paragraph::new(view.tree(id)?, node)
            .is_some_and(|p| failpoint::fails(&p.text(crate::wordml::Mode::Final)))
        {
            return Err(StetError::new(
                ErrorCode::Unsupported,
                "an injected failure of the copy",
            ));
        }
        let mut reader = Reader::new(view.tree(id)?);
        let snip = reader
            .read(node)
            .ok_or_else(|| internal("an original block is not an element"))?;
        let meta = Meta {
            used: reader.used,
            requires: reader.requires,
        };
        let top = self
            .write_all(tx, cx, snip, &meta)?
            .into_iter()
            .next()
            .ok_or_else(|| internal("an imported block was dropped whole"))?;
        Ok(Copied {
            node: top,
            source: BlockKey::new(Arc::clone(cx.src), node),
        })
    }

    /// Copies the characters `range` (an offset in the paragraph's `Final` text) of the original paragraph `para` as detached
    /// runs of the revised document's tree.
    pub(super) fn fragment(
        &mut self,
        tx: &mut ImportWriter<'_, '_>,
        cx: Cx<'_>,
        para: NodeId,
        range: Range<usize>,
    ) -> Result<Vec<NodeId>, StetError> {
        let cover = fragment::select(self.orig, cx.src, para, range)?;
        self.stats.math_dropped += cover.math;
        let view = self.orig.parts();
        let id = view
            .id(cx.src)
            .ok_or_else(|| internal("the original has no such part"))?;
        let mut reader = Reader::new(view.tree(id)?);
        let Some(snip) = reader.read(para) else {
            return Ok(Vec::new());
        };
        let meta = Meta {
            used: reader.used,
            requires: reader.requires,
        };
        let mut out = Vec::new();
        for piece in fragment::prune(snip, &cover) {
            out.extend(self.write_all(tx, cx, piece, &meta)?);
        }
        if out.is_empty() {
            return Err(internal("a fragment of an original paragraph is empty"));
        }
        Ok(out)
    }

    /// Rewrites a snip and writes what it becomes into the revised document's tree.
    fn write_all(
        &mut self,
        tx: &mut ImportWriter<'_, '_>,
        cx: Cx<'_>,
        snip: Snip,
        meta: &Meta,
    ) -> Result<Vec<NodeId>, StetError> {
        let mark = self.notes.hits.len();
        let kids = transform::run(self, tx, cx, snip)?;
        let mut tree = tx.story(cx.dst)?;
        snip::bind_used(&mut tree, &meta.used)?;
        let w = transform::w_ns(tree.tree());
        let mut out = Vec::new();
        for k in kids {
            if let snip::Kid::El(e) = k {
                out.push(snip::write(&mut tree, &e, &w, &meta.requires)?);
            }
        }
        // the notes that this copy names: it holds a reference to each
        let hits: Vec<(bool, i64)> = self.notes.hits.drain(mark..).collect();
        for (endnote, id) in hits {
            let new = self.notes.map.get(&(endnote, id)).copied().flatten();
            if let Some(note) = self
                .notes
                .fresh
                .iter_mut()
                .find(|n| n.endnote == endnote && Some(n.id) == new)
            {
                note.holder_part = Some(Arc::clone(cx.dst));
                note.holders.extend(out.iter().copied());
            }
        }
        Ok(out)
    }

    /// The notes copied since the last call: each is a `w:footnote` or `w:endnote` that has been added to the revised document's
    /// notes part, whose paragraphs the build marks as deleted.
    pub(super) fn take_new_notes(&mut self) -> Vec<NewNote> {
        std::mem::take(&mut self.notes.fresh)
    }

    /// Forgets a copied note that was not added (no copy that refers to it is in the story), so that a later reference copies
    /// it again.
    pub(super) fn forget_note(&mut self, note: &NewNote) {
        self.notes.forget(note.endnote, note.id);
    }
}
