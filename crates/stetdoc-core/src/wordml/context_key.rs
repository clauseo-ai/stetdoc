//! A key for the wrappers above a block.
//!
//! The text of a paragraph in each mode depends on the wrappers above it: an insertion, a deletion,
//! a row or a cell marked by a tracked change (`text_build::wrappers_above`). Moving a paragraph into
//! or out of such a wrapper, or marking its row, changes that text with no change to the paragraph's
//! bytes or position. The block scan keeps one key per entry, built from the same wrappers, so that
//! `Story::refresh` can report the blocks whose wrappers changed (`Refreshed::context_changed`).
//! The key is only compared between two scans of the same part in one process.

use super::text_build::ancestor_wrappers;
use super::text_model::WrapperKind;
use crate::xml::{NodeId, NodeRef};
use std::hash::{Hash, Hasher};

/// The key of a block with no wrapper above it.
pub(super) const ROOT: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a 64 as a [`Hasher`], so that the derived `Hash` of a wrapper kind and a node id feeds it.
struct Fnv(u64);

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

fn step(prev: u64, kind: WrapperKind, node: NodeId) -> u64 {
    let mut h = Fnv(prev);
    kind.hash(&mut h);
    node.hash(&mut h);
    h.finish()
}

/// The key of what stands inside `element`, given the key of what stands around it: unchanged
/// unless the element is a wrapper (a control, a revision, a row or cell marked by a revision, the
/// branch of an alternate content).
pub(super) fn enter(outer: u64, element: &NodeRef<'_>) -> u64 {
    ancestor_wrappers(element)
        .into_iter()
        .flatten()
        .fold(outer, |key, kind| step(key, kind, element.id()))
}
