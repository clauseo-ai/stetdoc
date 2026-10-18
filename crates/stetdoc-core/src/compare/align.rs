//! Block alignment: trim, unique-hash anchors, the longest increasing spine and the recursion between anchors
//! (design `compare.md`, 4.2). A stretch with no anchor goes to `align_gap`.
//!
//! Pure and `std` only: blocks come in as texts, hashes and bags, and a list of pairs comes out. The result is
//! deterministic: no map is iterated for output, ties go to the lower index of the original and then of the revised
//! side, and the hash is a function of the text alone.

use super::align_gap;
use super::text::{bag, hash_keyed, key_text};
use std::collections::HashMap;

/// What a block is. A pair is always of one kind, and the kind is part of the anchor key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A paragraph.
    Paragraph,
    /// A table.
    Table,
    /// A table row.
    Row,
}

impl Kind {
    /// The tag that goes into the hash, so a table never anchors a paragraph.
    fn tag(self) -> u8 {
        match self {
            Kind::Paragraph => 1,
            Kind::Table => 2,
            Kind::Row => 3,
        }
    }
}

/// A block as the aligner sees it.
#[derive(Clone, Debug)]
pub(super) struct Block {
    /// What it is.
    pub kind: Kind,
    /// The key text (white space collapsed, quotes straight).
    pub norm: String,
    /// The hash of the key text and the kind, or 0 for a blank, which never anchors.
    pub key: u64,
    /// The sorted word bag.
    pub bag: Vec<u64>,
}

impl Block {
    /// A block of `kind` with the comparison text `text`. For a row the text is the cells' texts joined by U+001F.
    pub(super) fn new(kind: Kind, text: &str) -> Block {
        let norm = key_text(text);
        let words = bag(&norm);
        let key = if words.is_empty() {
            0
        } else {
            hash_keyed(kind.tag(), &norm)
        };
        Block {
            kind,
            norm,
            key,
            bag: words,
        }
    }

    /// True if the two blocks are the same content as far as alignment goes: the same kind, and the same key text or
    /// both blank. A match of keys is verified against the strings, so a hash collision cannot pair two blocks.
    pub(super) fn same(&self, other: &Block) -> bool {
        self.kind == other.kind
            && self.key == other.key
            && (self.key == 0 || self.norm == other.norm)
    }

    /// True if the block has no word.
    pub(super) fn is_blank(&self) -> bool {
        self.bag.is_empty()
    }
}

/// How a pair was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum How {
    /// Equal at the ends of a stretch.
    Trim,
    /// A key that occurs once on each side, on the spine.
    Anchor,
    /// Paired by similarity.
    Similar,
}

/// A pair of an original block and a revised one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pair {
    /// The index in the original list.
    pub a: usize,
    /// The index in the revised list.
    pub b: usize,
    /// How it was found.
    pub how: How,
}

/// What the alignment did, for the tests and the figures of the design.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Stats {
    /// Stretches that had no anchor and went to similarity.
    pub stretches: usize,
    /// The cells (original blocks times revised blocks) of the stretches that the program ran on.
    pub cells: usize,
    /// Stretches that were over the cap and went through the inverted index.
    pub large: usize,
    /// How many times two blocks were scored.
    pub scored: usize,
}

/// The result of aligning two lists: the pairs in order of the original list, and what was done.
#[derive(Clone, Debug, Default)]
pub(super) struct Alignment {
    /// The pairs, ascending in both indexes.
    pub pairs: Vec<Pair>,
    /// The counters.
    pub stats: Stats,
}

/// A stretch: the half-open ranges of the two lists that are still unpaired.
#[derive(Clone, Copy, Debug)]
pub(super) struct Stretch {
    /// First original index.
    pub alo: usize,
    /// One past the last original index.
    pub ahi: usize,
    /// First revised index.
    pub blo: usize,
    /// One past the last revised index.
    pub bhi: usize,
}

/// Aligns the original blocks `a` with the revised blocks `b`.
pub(super) fn align(a: &[Block], b: &[Block]) -> Alignment {
    let mut out = Alignment::default();
    let mut stack = vec![Stretch {
        alo: 0,
        ahi: a.len(),
        blo: 0,
        bhi: b.len(),
    }];
    while let Some(mut s) = stack.pop() {
        trim(a, b, &mut s, &mut out.pairs);
        if s.alo >= s.ahi || s.blo >= s.bhi {
            continue;
        }
        let anchors = anchors(a, b, &s);
        if anchors.is_empty() {
            let gap = align_gap::pair_stretch(a, b, s, &mut out.stats);
            out.pairs.extend(gap.pairs);
            stack.extend(gap.rest);
            continue;
        }
        let (mut pa, mut pb) = (s.alo, s.blo);
        for &(i, j) in &anchors {
            out.pairs.push(Pair {
                a: i,
                b: j,
                how: How::Anchor,
            });
            stack.push(Stretch {
                alo: pa,
                ahi: i,
                blo: pb,
                bhi: j,
            });
            pa = i + 1;
            pb = j + 1;
        }
        stack.push(Stretch {
            alo: pa,
            ahi: s.ahi,
            blo: pb,
            bhi: s.bhi,
        });
    }
    out.pairs.sort_by_key(|p| (p.a, p.b));
    out
}

/// Pairs equal blocks from the front and from the back of the stretch, and shrinks it.
fn trim(a: &[Block], b: &[Block], s: &mut Stretch, pairs: &mut Vec<Pair>) {
    while s.alo < s.ahi && s.blo < s.bhi && same_at(a, b, s.alo, s.blo) {
        pairs.push(Pair {
            a: s.alo,
            b: s.blo,
            how: How::Trim,
        });
        s.alo += 1;
        s.blo += 1;
    }
    while s.alo < s.ahi && s.blo < s.bhi && same_at(a, b, s.ahi - 1, s.bhi - 1) {
        pairs.push(Pair {
            a: s.ahi - 1,
            b: s.bhi - 1,
            how: How::Trim,
        });
        s.ahi -= 1;
        s.bhi -= 1;
    }
}

/// True if block `i` of `a` and block `j` of `b` are the same content.
fn same_at(a: &[Block], b: &[Block], i: usize, j: usize) -> bool {
    match (a.get(i), b.get(j)) {
        (Some(x), Some(y)) => x.same(y),
        _ => false,
    }
}

/// The spine of the stretch: the longest increasing run of the pairs whose key occurs exactly once on each side and is
/// not blank.
fn anchors(a: &[Block], b: &[Block], s: &Stretch) -> Vec<(usize, usize)> {
    let mut in_a: HashMap<u64, (u32, usize)> = HashMap::new();
    for i in s.alo..s.ahi {
        if let Some(x) = a.get(i).filter(|x| x.key != 0) {
            let e = in_a.entry(x.key).or_insert((0, i));
            e.0 += 1;
        }
    }
    let mut in_b: HashMap<u64, (u32, usize)> = HashMap::new();
    for j in s.blo..s.bhi {
        if let Some(y) = b.get(j).filter(|y| y.key != 0) {
            let e = in_b.entry(y.key).or_insert((0, j));
            e.0 += 1;
        }
    }
    let mut pairs = Vec::new();
    for i in s.alo..s.ahi {
        let Some(x) = a.get(i).filter(|x| x.key != 0) else {
            continue;
        };
        if let (Some(&(1, _)), Some(&(1, j))) = (in_a.get(&x.key), in_b.get(&x.key))
            && same_at(a, b, i, j)
        {
            pairs.push((i, j));
        }
    }
    lis(&pairs)
}

/// The longest subsequence of `pairs` (ascending in the first index) that is ascending in the second, by patience
/// sorting. The result is the same for the same input.
pub(super) fn lis(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
    let mut tail_at: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (i, &(_, y)) in pairs.iter().enumerate() {
        let k = tails.partition_point(|&t| t < y);
        if k == tails.len() {
            tails.push(y);
            tail_at.push(i);
        } else {
            if let Some(t) = tails.get_mut(k) {
                *t = y;
            }
            if let Some(t) = tail_at.get_mut(k) {
                *t = i;
            }
        }
        if let Some(slot) = prev.get_mut(i) {
            *slot = k.checked_sub(1).and_then(|p| tail_at.get(p).copied());
        }
    }
    let mut out = Vec::new();
    let mut cur = tail_at.last().copied();
    while let Some(i) = cur {
        if let Some(&p) = pairs.get(i) {
            out.push(p);
        }
        cur = prev.get(i).copied().flatten();
    }
    out.reverse();
    out
}
