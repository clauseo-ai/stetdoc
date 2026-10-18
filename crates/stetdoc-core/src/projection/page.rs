//! The budget of a page: the estimate, the weight of a block, and the search for how many blocks fit.
//!
//! The budget is in tokens, estimated with no tokenizer (design 4.4): `ceil(ascii / 4) + non_ascii`, of the
//! whole text of a page, head and appendix included. A page is built greedily from the weights of its
//! blocks, rendered, measured with the real estimate, and corrected by a search, so that a page is as full
//! as the estimate allows and never over it unless one block alone is.

use super::build::scan::TopStat;
use crate::api::error::StetError;

/// The estimated tokens of a text: four ASCII characters are a token and every other character is one.
pub(crate) fn estimate(text: &str) -> u32 {
    let (mut ascii, mut other) = (0u32, 0u32);
    for c in text.chars() {
        if c.is_ascii() {
            ascii = ascii.saturating_add(1);
        } else {
            other = other.saturating_add(1);
        }
    }
    ascii.div_ceil(4).saturating_add(other)
}

/// The weight of a top before it is rendered: a quarter of its characters, and four for the overhead of
/// each line.
pub(crate) fn weight(stat: &TopStat) -> u32 {
    stat.chars
        .div_ceil(4)
        .saturating_add(stat.lines.saturating_mul(4))
        .max(4)
}

/// The largest `n` between 1 and `most` for which `tokens(n)` is within `budget`, or 1 when none is. A budget
/// of zero has no limit. `guess` is where the search starts, from the weights, and `tokens` renders the
/// first `n` blocks and measures them. The estimate grows with `n`, so the search gallops from the guess and
/// then halves.
pub(crate) fn fit(
    most: usize,
    budget: u32,
    guess: usize,
    tokens: &mut dyn FnMut(usize) -> Result<u32, StetError>,
) -> Result<usize, StetError> {
    if most <= 1 || budget == 0 {
        return Ok(most.min(if budget == 0 { most } else { 1 }));
    }
    let mut fits = |n: usize| -> Result<bool, StetError> { Ok(tokens(n)? <= budget) };
    let start = guess.clamp(1, most);
    let (mut lo, mut hi); // lo fits (or is 1), hi does not fit (or is most + 1)
    if fits(start)? {
        lo = start;
        let mut step = 1usize;
        hi = most + 1;
        while lo < most {
            let next = (lo + step).min(most);
            if fits(next)? {
                lo = next;
                step = step.saturating_mul(2);
            } else {
                hi = next;
                break;
            }
        }
    } else {
        hi = start;
        lo = 1;
        let mut step = 1usize;
        while hi > 1 {
            let next = hi.saturating_sub(step).max(1);
            if next == 1 || fits(next)? {
                lo = next;
                break;
            }
            hi = next;
            step = step.saturating_mul(2);
        }
    }
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if fits(mid)? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(lo)
}

/// How many of the leading `weights` fit in `budget` tokens, with at least one: the guess that `fit` starts
/// from. A budget of zero holds them all.
pub(crate) fn greedy(weights: impl Iterator<Item = u32>, budget: u32, overhead: u32) -> usize {
    let mut used = overhead;
    let mut n = 0usize;
    for w in weights {
        if budget != 0 && n > 0 && used.saturating_add(w) > budget {
            break;
        }
        used = used.saturating_add(w);
        n += 1;
    }
    n
}
