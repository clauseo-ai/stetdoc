//! Similarity of two blocks: the multiset Dice coefficient of their word bags, and the floor a pair has to clear.
//!
//! Pure, `std` only. The score of two bags is `2 |A ∩ B| / (|A| + |B|)`, two empty bags score 1.0 (two blanks are the same
//! content), and an empty bag against a full one scores 0. The floor of a pair in a stretch is 0.35 plus 0.3 times the
//! difference of the two relative positions in the stretch, so that a pair that is far off the diagonal has to be a
//! clear match (design `compare.md`, 4.3).

/// The base floor of a similarity pair.
pub(super) const FLOOR: f64 = 0.35;

/// The weight of the displacement in the floor.
pub(super) const LOCALITY: f64 = 0.3;

/// The size of the multiset intersection of two sorted bags.
pub(super) fn common(a: &[u64], b: &[u64]) -> usize {
    let (mut i, mut j, mut n) = (0usize, 0usize, 0usize);
    while let (Some(x), Some(y)) = (a.get(i), b.get(j)) {
        match x.cmp(y) {
            std::cmp::Ordering::Equal => {
                n += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    n
}

/// The multiset Dice coefficient of two sorted bags.
pub(super) fn dice(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    2.0 * as_f64(common(a, b)) / as_f64(a.len() + b.len())
}

/// A count as a float. Counts here are far below 2^52, so the conversion is exact.
pub(super) fn as_f64(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}

/// The floor that the pair (`i` of `m`, `j` of `n`) has to clear in a stretch of `m` blocks against `n`.
pub(super) fn floor_at(i: usize, m: usize, j: usize, n: usize) -> f64 {
    let ri = (as_f64(i) + 0.5) / as_f64(m.max(1));
    let rj = (as_f64(j) + 0.5) / as_f64(n.max(1));
    FLOOR + LOCALITY * (ri - rj).abs()
}
