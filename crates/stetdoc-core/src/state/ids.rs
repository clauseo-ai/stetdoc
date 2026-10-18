//! Identifiers that the state mints, and the sets of ids a document already uses.
//!
//! A new `w14:paraId`, `w14:textId` or `w16cid:durableId` is a hash of the namespace, the new
//! state version, the kind, the part and the ordinal of the request, bumped past every id the
//! document uses. It comes from inputs only, so the same edits on the same bytes mint the same ids,
//! and no clock or random number generator is involved.

use super::derived::Dep;
use super::errors::{internal, limit};
use super::tx::Tx;
use super::view::PartsView;
use crate::api::error::StetError;
use crate::refs::Hasher;
use crate::wordml::{IdKind, existing_ids, id_parts};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::sync::Arc;

/// The largest value of a durable id: `w14:paraId` must be below `0x8000_0000`.
const MAX_ID: u32 = 0x7FFF_FFFF;

/// The largest annotation id: `w:id` is a signed 32-bit number, and Word does not read a larger one.
const MAX_ANNOTATION_ID: u32 = 0x7FFF_FFFF;

/// The ids of one kind that a document uses, sorted and without repeats.
pub(crate) struct IdSet(Vec<u32>);

impl IdSet {
    /// True if the document uses this id.
    pub(crate) fn contains(&self, id: u32) -> bool {
        self.0.binary_search(&id).is_ok()
    }

    /// The highest id in use, which `revisions` adds one to for a revision or comment id.
    pub(crate) fn max(&self) -> Option<u32> {
        self.0.last().copied()
    }

    /// How many ids are in use.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

fn kind_code(kind: IdKind) -> u64 {
    match kind {
        IdKind::ParaId => 1,
        IdKind::TextId => 2,
        IdKind::CommentDurableId => 3,
        IdKind::AnnotationId => 4,
    }
}

impl<'a> PartsView<'a> {
    /// The ids of `kind` in use, read from every part `wordml::id_parts` names, both branches of an
    /// `mc:AlternateContent` included. The set is a cache over those parts and the package.
    pub(crate) fn ids(&self, kind: IdKind) -> Result<Arc<IdSet>, StetError> {
        let main = self.base.main_part()?;
        let list = self.story_specs()?;
        let rels = self.rels(main)?;
        let mut deps = vec![Dep::Package, Dep::Part(main)];
        let mut parts = Vec::new();
        for name in id_parts(kind, &list.0, &rels) {
            if let Some(id) = self.id(&name) {
                deps.push(Dep::Part(id));
                parts.push(id);
            }
        }
        let build = |_: Option<&IdSet>| -> Result<IdSet, StetError> {
            let mut found = Vec::new();
            for id in &parts {
                existing_ids(kind, self.tree(*id)?, &mut found);
            }
            found.sort_unstable();
            found.dedup();
            Ok(IdSet(found))
        };
        self.derived(kind_code(kind), &deps, &build)
    }
}

/// The annotation ids that a transaction has handed out: they are the numbers from `first` up to, and
/// not including, `next`.
#[derive(Clone, Copy)]
struct Annotations {
    first: u32,
    next: u32,
}

/// What a transaction has minted: the ordinal of the next request per kind and part, and the ids
/// it has given out, which later requests of the same transaction must avoid.
#[derive(Default)]
pub(super) struct IdMint {
    ordinals: HashMap<(u64, Arc<str>), u32>,
    minted: HashMap<u64, HashSet<u32>>,
    annotations: Option<Annotations>,
    /// The annotation ids that were noted as copies of a revision that already existed.
    copies: Vec<u32>,
}

impl Tx<'_> {
    /// The next annotation id: the `w:id` of a revision, a comment, a bookmark or a range marker.
    /// Revisions and comments share it, so they never hand out the same number.
    ///
    /// The first call of a transaction gives the highest `w:id` that the transaction's view of the
    /// document uses plus one, or 0 when it uses none, and each later call gives one more. The view
    /// includes the parts that `add_part` brought in and the edits staged so far, but only at the
    /// first call. A part that is added after it takes its annotation ids from this counter, or has
    /// none, because the counter does not look again. So the ids a transaction has made are
    /// exactly `minted_annotation_ids()`, with nothing else between them, and a dry run and the real
    /// edit at the same base make the same ids. It is `limit_exceeded` once the next id would pass
    /// 2,147,483,647, and then nothing has changed.
    pub(crate) fn next_annotation_id(&mut self) -> Result<u32, StetError> {
        let next = match self.mint.annotations {
            Some(made) => made.next,
            None => match self.parts().ids(IdKind::AnnotationId)?.max() {
                None => 0,
                Some(highest) => highest.saturating_add(1),
            },
        };
        if next > MAX_ANNOTATION_ID {
            return Err(limit(format!(
                "this document's annotation ids (revisions, comments and bookmarks) have reached \
                 {MAX_ANNOTATION_ID}, the largest that Word reads, so no new one can be made"
            )));
        }
        let first = self.mint.annotations.map_or(next, |made| made.first);
        self.mint.annotations = Some(Annotations {
            first,
            next: next + 1,
        });
        Ok(next)
    }

    /// The annotation ids that this transaction has made with `next_annotation_id`, as the range from
    /// the first to one past the last. It is empty when none was made. Every id in it was made
    /// here, and no other id is in it, so an element whose `w:id` is in the range got that id from this
    /// transaction, whichever operation or compiler made it. Some of those ids are on copies of a
    /// revision that already existed, and not on new revisions: `copied_annotation_ids` lists them.
    pub(crate) fn minted_annotation_ids(&self) -> Range<u32> {
        self.mint
            .annotations
            .map_or(0..0, |made| made.first..made.next)
    }

    /// Notes that `id`, one of the ids that this transaction made, is on a copy of a revision that
    /// already existed (the tail of a split `w:ins` or `w:del`, the copy of a pending `w:rPrChange`
    /// that a carve makes) and not on a new revision, so that it is never extended or edited in place.
    /// The note is for `revisions`, which makes it with one compiler and needs it with the next one over
    /// the same transaction. It has no effect on the commit, the audit or the digest, and it goes with
    /// the transaction. An id that this transaction did not make is `internal`, and then nothing is
    /// noted.
    pub(crate) fn note_copied_annotation_id(&mut self, id: u32) -> Result<(), StetError> {
        if !self.minted_annotation_ids().contains(&id) {
            return Err(internal(format!(
                "annotation id {id} was noted as a copy, and this transaction did not make it"
            )));
        }
        self.mint.copies.push(id);
        Ok(())
    }

    /// The annotation ids noted as copies of a revision that already existed, in the order they were
    /// noted. Every one is in `minted_annotation_ids`.
    pub(crate) fn copied_annotation_ids(&self) -> &[u32] {
        &self.mint.copies
    }

    /// A new id of `kind` for an element of `part`: a hash of the namespace, the new version, the
    /// kind, the part and the ordinal of this request, bumped by one (wrapping within
    /// `1..=0x7FFF_FFFF`) until no id in the document, and none this transaction minted, equals it.
    /// A dry run and the real edit at the same version mint the same ids. Ids that a deletion
    /// frees are not reused. `IdKind::AnnotationId` is not minted here: `next_annotation_id` makes
    /// those, one above the highest in use.
    pub(crate) fn mint_id(&mut self, kind: IdKind, part: &str) -> Result<u32, StetError> {
        if kind == IdKind::AnnotationId {
            return Err(internal(
                "annotation ids come from Tx::next_annotation_id, and mint_id does not make them",
            ));
        }
        let code = kind_code(kind);
        let name = self
            .index
            .id(part)
            .and_then(|id| self.index.name(id))
            .cloned()
            .ok_or_else(|| {
                internal(format!(
                    "an id was asked for in {part:?}, which is not a part"
                ))
            })?;
        let used = self.base().parts().ids(kind)?;
        let ordinal = {
            let next = self
                .mint
                .ordinals
                .entry((code, Arc::clone(&name)))
                .or_insert(0);
            let ordinal = *next;
            *next = next.saturating_add(1);
            ordinal
        };
        let version = u64::from(self.base().base_token().version()) + 1;
        let namespace = self.base().base_token().namespace();
        let mut hasher = Hasher::new("stetdoc/id");
        hasher
            .field(namespace.as_bytes())
            .number(version)
            .number(code)
            .field(name.as_bytes())
            .number(u64::from(ordinal));
        let digest = hasher.finish();
        let first = digest
            .first_chunk::<4>()
            .map(|b| u32::from_le_bytes(*b))
            .unwrap_or(0);
        let minted = self.mint.minted.entry(code).or_default();
        // A free id exists among the next `taken + 1` candidates unless every id is in use.
        let rounds = used.len().saturating_add(minted.len()).saturating_add(1);
        let free = next_free(first, rounds, |id| {
            used.contains(id) || minted.contains(&id)
        })
        .ok_or_else(|| limit("every durable id of this kind is in use"))?;
        minted.insert(free);
        Ok(free)
    }
}

/// The first id, from `first & MAX_ID` upward and wrapping within `1..=MAX_ID`, that `taken` does
/// not report, looking at no more than `rounds + 1` candidates. Zero is never free.
fn next_free(first: u32, rounds: usize, taken: impl Fn(u32) -> bool) -> Option<u32> {
    let mut candidate = first & MAX_ID;
    for _ in 0..=rounds {
        if candidate != 0 && !taken(candidate) {
            return Some(candidate);
        }
        candidate = if candidate >= MAX_ID {
            1
        } else {
            candidate + 1
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_free_candidate_is_chosen_and_zero_is_never_free() {
        assert_eq!(next_free(5, 10, |_| false), Some(5));
        assert_eq!(next_free(0, 10, |_| false), Some(1));
        assert_eq!(
            next_free(0x8000_0007, 10, |_| false),
            Some(7),
            "the high bit is masked off"
        );
        assert_eq!(
            next_free(MAX_ID, 10, |id| id == MAX_ID),
            Some(1),
            "it wraps to 1, not 0"
        );
        assert_eq!(next_free(MAX_ID - 1, 10, |id| id >= MAX_ID - 1), Some(1));
    }

    #[test]
    fn ten_thousand_taken_candidates_are_walked_past() {
        let taken = |id: u32| (1000..11_000).contains(&id);
        assert_eq!(next_free(1000, 10_000, taken), Some(11_000));
        assert_eq!(next_free(10_999, 10_000, taken), Some(11_000));
        assert_eq!(next_free(1000, 5, taken), None, "the search is bounded");
    }

    #[test]
    fn a_search_that_finds_everything_taken_gives_up() {
        assert_eq!(next_free(1, 100, |_| true), None);
    }
}
