//! Block fingerprints: `Story::fingerprints`.
//!
//! The definition is frozen by `FINGERPRINT_VERSION` and is `docs/design/wordml.md` 3.4, word for
//! word. FNV-1a 64. A paragraph hashes the kind byte 1, then the UTF-8 of every text and CDATA node
//! below it in document order, decoded as `NodeRef::text` decodes it. A table hashes the kind byte 2,
//! and a row the kind byte 3, then the 8 little-endian bytes of the fingerprint of every scan entry
//! whose parent is it, in scan order. Nothing else enters a fingerprint: not the text model, not the
//! field pass, not the opaque entries.

use super::story::{BlockKind, Story};
use crate::xml::{NodeKind, NodeRef, SpanTree};

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn feed(hash: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *hash ^= u64::from(b);
        *hash = hash.wrapping_mul(FNV_PRIME);
    }
}

/// The fingerprint of a paragraph: its kind byte and then every text and CDATA node below it.
fn paragraph(p: &NodeRef<'_>) -> u64 {
    let mut hash = FNV_OFFSET;
    feed(&mut hash, &[1]);
    let mut stack = vec![p.children()];
    while let Some(top) = stack.last_mut() {
        match top.next() {
            None => {
                stack.pop();
            }
            Some(n) => match n.kind() {
                NodeKind::Text | NodeKind::CData => feed(&mut hash, n.text().as_bytes()),
                NodeKind::Element => stack.push(n.children()),
                _ => {}
            },
        }
    }
    hash
}

impl Story {
    /// One fingerprint for each entry of [`Story::blocks`], in the same order. The tree must be the
    /// one the story was scanned from.
    pub fn fingerprints(&self, tree: &SpanTree) -> Vec<u64> {
        let n = self.entries.len();
        let mut out: Vec<u64> = self
            .entries
            .iter()
            .map(|e| match e.kind {
                BlockKind::Paragraph => paragraph(&tree.node_ref(e.node)),
                BlockKind::Table | BlockKind::Row => 0,
            })
            .collect();
        // The children of each entry in scan order, as one flat list with offsets.
        let mut offsets = vec![0u32; n + 1];
        for e in &self.entries {
            if let Some(slot) = e.parent.and_then(|p| offsets.get_mut(p as usize + 1)) {
                *slot += 1;
            }
        }
        let mut total = 0u32;
        for o in &mut offsets {
            total += *o;
            *o = total;
        }
        let mut fill = offsets.clone();
        let mut kids = vec![0u32; total as usize];
        for (i, e) in self.entries.iter().enumerate() {
            let Some(parent) = e.parent else { continue };
            if let Some(at) = fill.get_mut(parent as usize) {
                if let (Some(slot), Ok(child)) = (kids.get_mut(*at as usize), u32::try_from(i)) {
                    *slot = child;
                }
                *at += 1;
            }
        }
        // Children come after their parent, so going backwards sees every child first.
        for i in (0..n).rev() {
            let Some(entry) = self.entries.get(i) else {
                continue;
            };
            let kind_byte = match entry.kind {
                BlockKind::Paragraph => continue,
                BlockKind::Table => 2u8,
                BlockKind::Row => 3u8,
            };
            let mut hash = FNV_OFFSET;
            feed(&mut hash, &[kind_byte]);
            let from = offsets.get(i).copied().unwrap_or(0) as usize;
            let to = offsets.get(i + 1).copied().unwrap_or(0) as usize;
            for &child in kids.get(from..to).unwrap_or(&[]) {
                let fp = out.get(child as usize).copied().unwrap_or(0);
                feed(&mut hash, &fp.to_le_bytes());
            }
            if let Some(slot) = out.get_mut(i) {
                *slot = hash;
            }
        }
        out
    }
}
