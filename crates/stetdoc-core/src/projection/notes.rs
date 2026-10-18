//! Margin notes that come from the index: what a citation in the text came to, and what cites a clause.
//!
//! A mention such as `clause 9.1` is found in a paragraph's `Final` text and resolved against the clause table
//! (design 7.4), and a note after the block says where it points and how sure that is, so a reader who sees a
//! reference to a clause far away does not have to look for it. The notes are for the document as it will be,
//! so a view of `Original` has none. A file never holds one.

use super::model::{Margin, MentionState, Target};
use crate::api::error::StetError;
use crate::index::{Edge, EdgeKind, Edges, Index, State};
use crate::refs::{BlockKey, BlockRef};
use crate::state::PartsView;
use std::sync::Arc;

/// The most candidates a note lists. The rest are counted.
const LISTED: usize = 3;

/// The most citing blocks a note lists. The rest are counted.
const CITING: usize = 12;

/// The index and the edges of a document, for the notes of one page.
pub(super) struct Notes {
    index: Arc<Index>,
    edges: Arc<Edges>,
}

fn state_of(state: State) -> MentionState {
    match state {
        State::Resolved => MentionState::Resolved,
        State::Relative => MentionState::Relative,
        State::Ambiguous => MentionState::Ambiguous,
        State::Outside => MentionState::Outside,
        State::Unresolved => MentionState::Unresolved,
    }
}

impl Notes {
    /// The index of the document and its edges, built on first use and cached by `state`.
    pub(super) fn of(parts: &PartsView<'_>) -> Result<Notes, StetError> {
        Ok(Notes {
            index: parts.doc_index()?,
            edges: parts.edges()?,
        })
    }

    /// A clause as a note shows it: its ref and enough of it to recognize.
    fn target(&self, parts: &PartsView<'_>, key: &BlockKey) -> Option<Target> {
        let clause = self.index.clause_of(key)?;
        Some(Target {
            r#ref: parts.ref_of(key)?,
            text: clause.heading().into_boxed_str(),
        })
    }

    fn mention(&self, parts: &PartsView<'_>, edge: &Edge) -> Margin {
        let candidates: Vec<Target> = edge
            .to
            .iter()
            .filter_map(|k| self.target(parts, k))
            .collect();
        let more = candidates.len().saturating_sub(LISTED);
        Margin::Mention {
            text: edge.text.clone().into_boxed_str(),
            state: state_of(edge.state),
            targets: candidates.into_iter().take(LISTED).collect(),
            more: u32::try_from(more).unwrap_or(u32::MAX),
        }
    }

    /// The notes of the mentions in one paragraph, in the order of the text, each once.
    pub(super) fn mentions(&self, parts: &PartsView<'_>, key: &BlockKey) -> Vec<Margin> {
        let mut out: Vec<Margin> = Vec::new();
        for edge in self.edges.outbound(key) {
            if edge.kind != EdgeKind::Prose {
                continue;
            }
            let note = self.mention(parts, edge);
            if !out.contains(&note) {
                out.push(note);
            }
        }
        out
    }

    /// The note that says which blocks cite a clause, for a view of that clause.
    pub(super) fn cited_by(&self, parts: &PartsView<'_>, key: &BlockKey) -> Option<Margin> {
        let mut from: Vec<BlockRef> = Vec::new();
        for edge in self.edges.inbound(key) {
            if let Some(r) = parts.ref_of(&edge.from)
                && !from.contains(&r)
            {
                from.push(r);
            }
        }
        if from.is_empty() {
            return None;
        }
        let more = from.len().saturating_sub(CITING);
        from.truncate(CITING);
        Some(Margin::CitedBy {
            from,
            more: u32::try_from(more).unwrap_or(u32::MAX),
        })
    }
}
