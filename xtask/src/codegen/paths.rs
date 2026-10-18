//! What can follow what in a minimal DFA: which symbols occur at most once in every accepted
//! sequence, and whether the children can be sorted into one order.

use super::automaton::{Dfa, NONE};
use std::collections::BTreeMap;

/// The states of a DFA that lie on some accepted sequence, and how they reach each other.
pub(super) struct Paths<'a> {
    dfa: &'a Dfa,
    useful: Vec<bool>,
    /// `reach[q]`: the useful states reachable from useful `q` through useful states, `q`
    /// included.
    reach: Vec<Vec<bool>>,
}

impl<'a> Paths<'a> {
    pub(super) fn new(dfa: &'a Dfa) -> Paths<'a> {
        let states = dfa.states();
        let cols = dfa.cols();
        let succ = |q: usize| -> Vec<usize> {
            (0..cols)
                .filter_map(|c| {
                    let t = dfa.next(q, c);
                    (t != NONE).then_some(usize::from(t))
                })
                .collect()
        };
        let mut forward = vec![false; states];
        let mut stack = Vec::new();
        if let Some(start) = forward.first_mut() {
            *start = true;
            stack.push(0usize);
        }
        while let Some(q) = stack.pop() {
            for t in succ(q) {
                if let Some(seen) = forward.get_mut(t)
                    && !*seen
                {
                    *seen = true;
                    stack.push(t);
                }
            }
        }
        let mut finishes = dfa.accept.clone();
        loop {
            let mut changed = false;
            for q in 0..states {
                if !finishes.get(q).copied().unwrap_or(false)
                    && succ(q)
                        .iter()
                        .any(|t| finishes.get(*t).copied().unwrap_or(false))
                    && let Some(f) = finishes.get_mut(q)
                {
                    *f = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let useful: Vec<bool> = (0..states)
            .map(|q| {
                forward.get(q).copied().unwrap_or(false)
                    && finishes.get(q).copied().unwrap_or(false)
            })
            .collect();
        let is_useful = |q: usize| useful.get(q).copied().unwrap_or(false);
        let reach: Vec<Vec<bool>> = (0..states)
            .map(|q| {
                let mut seen = vec![false; states];
                if is_useful(q) {
                    if let Some(s) = seen.get_mut(q) {
                        *s = true;
                    }
                    let mut stack = vec![q];
                    while let Some(x) = stack.pop() {
                        for t in succ(x) {
                            if is_useful(t)
                                && let Some(s) = seen.get_mut(t)
                                && !*s
                            {
                                *s = true;
                                stack.push(t);
                            }
                        }
                    }
                }
                seen
            })
            .collect();
        Paths { dfa, useful, reach }
    }

    fn is_useful(&self, q: usize) -> bool {
        self.useful.get(q).copied().unwrap_or(false)
    }

    /// The useful state that reading symbol `sym` in `q` leads to.
    fn edge(&self, q: usize, sym: usize) -> Option<usize> {
        let t = self.dfa.next(q, sym);
        (t != NONE && self.is_useful(usize::from(t)) && self.is_useful(q)).then_some(usize::from(t))
    }

    /// Whether some accepted sequence reads `a` and then, later, `b`.
    fn then(&self, a: usize, b: usize) -> bool {
        (0..self.dfa.states()).any(|q1| {
            self.edge(q1, a).is_some_and(|t1| {
                self.reach.get(t1).is_some_and(|r| {
                    r.iter()
                        .enumerate()
                        .any(|(q2, reachable)| *reachable && self.edge(q2, b).is_some())
                })
            })
        })
    }
}

/// For each alphabet symbol: does it occur at most once in every accepted sequence?
pub(super) fn max_once(paths: &Paths<'_>) -> Vec<bool> {
    (0..paths.dfa.alphabet.len())
        .map(|sym| !paths.then(sym, sym))
        .collect()
}

/// Whether the children can be sorted into one order: no wildcard, at least two symbols, no
/// more than three symbols sharing a rank, and any two symbols that never occur together (the
/// alternatives of a choice) share a rank, so that sorting by rank never swaps them. Swapping
/// alternatives that exclude each other would only change which one is blamed.
pub(super) fn sortable(paths: &Paths<'_>, rank: &[u16]) -> bool {
    let dfa = paths.dfa;
    let n = dfa.alphabet.len();
    let mut ties: BTreeMap<u16, usize> = BTreeMap::new();
    for r in rank {
        *ties.entry(*r).or_insert(0) += 1;
    }
    n >= 2
        && !dfa.any
        && ties.values().all(|t| *t <= 3)
        && (0..n).all(|a| {
            ((a + 1)..n).all(|b| paths.then(a, b) || paths.then(b, a) || rank.get(a) == rank.get(b))
        })
}
