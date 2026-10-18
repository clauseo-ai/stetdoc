//! Pairing a stretch that has no anchor: a weighted longest common subsequence for a small stretch, and the
//! inverted-index path for a large one (design `compare.md`, 4.3 and 4.4).
//!
//! Pure and `std` only. The program maximizes the sum of the Dice scores of an order-preserving pairing in which every
//! pair clears the floor of its position. A stretch of more than [`CELLS_MAX`] cells is paired through the words that
//! occur in at most [`INDEX_CAP`] blocks of the revised side, and the stretches between those pairs go round again.

use super::align::{Block, How, Pair, Stats, Stretch, lis};
use super::similarity::{dice, floor_at};
use std::collections::HashMap;

/// The most cells (original blocks times revised blocks) that one stretch may have for the program. It is the cap of
/// the word diff (`DP_CELLS_MAX`), so the two have one bound.
pub(super) const CELLS_MAX: usize = 250_000;

/// A word that occurs in more blocks of a stretch than this is not a key of the inverted index. The moves pass uses the
/// same cap.
pub(super) const INDEX_CAP: usize = 8;

/// What pairing a stretch gave: the pairs, and the stretches that are left to align again.
pub(super) struct Gap {
    /// The pairs found.
    pub pairs: Vec<Pair>,
    /// Stretches between the pairs of the large path, to be aligned again from the trim.
    pub rest: Vec<Stretch>,
}

/// A rectangular grid of copyable values, read and written without indexing.
struct Grid<T> {
    width: usize,
    cells: Vec<T>,
}

impl<T: Copy + Default> Grid<T> {
    fn new(rows: usize, width: usize) -> Grid<T> {
        Grid {
            width,
            cells: vec![T::default(); rows * width],
        }
    }

    fn get(&self, i: usize, j: usize) -> T {
        self.cells
            .get(i * self.width + j)
            .copied()
            .unwrap_or_default()
    }

    fn set(&mut self, i: usize, j: usize, v: T) {
        if let Some(c) = self.cells.get_mut(i * self.width + j) {
            *c = v;
        }
    }
}

/// The score of block `i` of `a` against block `j` of `b`, or 0 for two blocks of different kinds.
fn score(a: &[Block], b: &[Block], i: usize, j: usize, stats: &mut Stats) -> f64 {
    match (a.get(i), b.get(j)) {
        (Some(x), Some(y)) if x.kind == y.kind => {
            stats.scored += 1;
            dice(&x.bag, &y.bag)
        }
        _ => 0.0,
    }
}

/// Pairs one stretch that has no anchor.
pub(super) fn pair_stretch(a: &[Block], b: &[Block], s: Stretch, stats: &mut Stats) -> Gap {
    let (m, n) = (s.ahi - s.alo, s.bhi - s.blo);
    stats.stretches += 1;
    if m.saturating_mul(n) <= CELLS_MAX {
        stats.cells += m * n;
        Gap {
            pairs: program(a, b, s, stats),
            rest: Vec::new(),
        }
    } else {
        stats.large += 1;
        indexed(a, b, s, stats)
    }
}

/// The weighted longest common subsequence of a small stretch.
fn program(a: &[Block], b: &[Block], s: Stretch, stats: &mut Stats) -> Vec<Pair> {
    let (m, n) = (s.ahi - s.alo, s.bhi - s.blo);
    let mut sims: Grid<f32> = Grid::new(m, n);
    for i in 0..m {
        for j in 0..n {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a score between 0 and 1 fits an f32"
            )]
            let v = score(a, b, s.alo + i, s.blo + j, stats) as f32;
            sims.set(i, j, v);
        }
    }
    // best[i][j]: the best total for the first i original and the first j revised blocks.
    let mut best: Grid<f64> = Grid::new(m + 1, n + 1);
    // choice: 0 skip an original block, 1 skip a revised block, 2 pair them.
    let mut choice: Grid<u8> = Grid::new(m + 1, n + 1);
    for i in 1..=m {
        for j in 1..=n {
            let up = best.get(i - 1, j);
            let left = best.get(i, j - 1);
            let (mut top, mut how) = if left > up { (left, 1u8) } else { (up, 0u8) };
            let sim = f64::from(sims.get(i - 1, j - 1));
            if sim >= floor_at(i - 1, m, j - 1, n) {
                let diag = best.get(i - 1, j - 1) + sim;
                if diag >= top {
                    top = diag;
                    how = 2;
                }
            }
            best.set(i, j, top);
            choice.set(i, j, how);
        }
    }
    let mut pairs = Vec::new();
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        match choice.get(i, j) {
            2 => {
                pairs.push(Pair {
                    a: s.alo + i - 1,
                    b: s.blo + j - 1,
                    how: How::Similar,
                });
                i -= 1;
                j -= 1;
            }
            0 => i -= 1,
            _ => j -= 1,
        }
    }
    pairs.reverse();
    pairs
}

/// A large stretch: each original block takes the best revised block among those that share a word of the index, and
/// the longest increasing run of those pairs is kept.
fn indexed(a: &[Block], b: &[Block], s: Stretch, stats: &mut Stats) -> Gap {
    let (m, n) = (s.ahi - s.alo, s.bhi - s.blo);
    let mut posting: HashMap<u64, Vec<usize>> = HashMap::new();
    for j in s.blo..s.bhi {
        let Some(y) = b.get(j) else { continue };
        let mut last = None;
        for &t in &y.bag {
            if last != Some(t) {
                let list = posting.entry(t).or_default();
                if list.len() <= INDEX_CAP {
                    list.push(j);
                }
                last = Some(t);
            }
        }
    }
    let mut picked: Vec<(usize, usize)> = Vec::new();
    for i in s.alo..s.ahi {
        let Some(x) = a.get(i) else { continue };
        let mut cand: Vec<usize> = Vec::new();
        let mut last = None;
        for &t in &x.bag {
            if last != Some(t) {
                last = Some(t);
                if let Some(list) = posting.get(&t).filter(|l| l.len() <= INDEX_CAP) {
                    cand.extend(list.iter().copied());
                }
            }
        }
        cand.sort_unstable();
        cand.dedup();
        let mut top: Option<(usize, f64)> = None;
        for j in cand {
            let sc = score(a, b, i, j, stats);
            if top.is_none_or(|(_, t)| sc > t) {
                top = Some((j, sc));
            }
        }
        if let Some((j, sc)) = top
            && sc >= floor_at(i - s.alo, m, j - s.blo, n)
        {
            picked.push((i, j));
        }
    }
    let spine = lis(&picked);
    let mut pairs = Vec::new();
    let mut rest = Vec::new();
    if spine.is_empty() {
        return Gap { pairs, rest };
    }
    let (mut pa, mut pb) = (s.alo, s.blo);
    for &(i, j) in &spine {
        pairs.push(Pair {
            a: i,
            b: j,
            how: How::Similar,
        });
        rest.push(Stretch {
            alo: pa,
            ahi: i,
            blo: pb,
            bhi: j,
        });
        pa = i + 1;
        pb = j + 1;
    }
    rest.push(Stretch {
        alo: pa,
        ahi: s.ahi,
        blo: pb,
        bhi: s.bhi,
    });
    Gap { pairs, rest }
}
