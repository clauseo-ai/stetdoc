//! Moves, splits and merges: what the alignment left over (design `compare.md`, 4.6).
//!
//! After every container of a story is paired, the unpaired non-blank blocks of the whole story, from any container of a
//! paired structure, form two pools. A deleted block and an inserted block that are at least 0.8 alike, and have five
//! words or more, are a **move**: written as a deletion and an insertion, and listed in the receipt. A block inside an
//! unpaired table or row is not in the pools (the table or the row is the unit). Then a paired paragraph with unpaired
//! neighbours that together cover its words is a **split** (one original paragraph, several revised ones) or a **merge**.
//! The pass uses the index cap of the large stretch, so it stays linear on boilerplate.

use super::align::Kind;
use super::align_gap::INDEX_CAP;
use super::model::{Body, StoryModel};
use super::similarity::{as_f64, common, dice};
use super::tables::StoryPairing;
use super::text::word_count;
use std::collections::HashMap;

/// The least score of a move (the value of `WmlComparer` and of Docxodus).
pub(super) const MOVE_FLOOR: f64 = 0.8;

/// The fewest words a block needs to be a move: a short line that appears elsewhere is a coincidence.
pub(super) const MOVE_WORDS: usize = 5;

/// The share of the original paragraph's words that the revised paragraphs of a split must cover, and the same for the
/// original paragraphs of a merge.
pub(super) const GROUP_COVER: f64 = 0.8;

/// The share of the words that each added part must contribute.
pub(super) const GROUP_SHARE: f64 = 0.10;

/// The share of each part's own words that must be in the paragraph it belongs to with, so that a neighbour that merely
/// shares function words is not taken for a part.
pub(super) const GROUP_PRECISION: f64 = 0.6;

/// The share of the words that one part may cover alone. A part that covers this much of the paragraph is that paragraph with
/// something added next to it, not a piece of it: the paired paragraph and an inserted neighbour are not a split.
pub(super) const GROUP_SOLO: f64 = 0.8;

/// The most paragraphs in one side of a group.
pub(super) const GROUP_MAX: usize = 3;

/// A block of a story pairing: the container pairing and the index in the container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Addr {
    /// The index into `StoryPairing::containers`.
    pub pairing: usize,
    /// The index of the block in the container of that pairing, on the side the field names.
    pub index: usize,
}

/// A block deleted where it was and inserted where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Move {
    /// The original block.
    pub a: Addr,
    /// The revised block.
    pub b: Addr,
}

/// A split (one original paragraph, several revised) or a merge (the reverse), inside one container pairing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Group {
    /// The container pairing.
    pub pairing: usize,
    /// The original paragraphs, consecutive.
    pub a: Vec<usize>,
    /// The revised paragraphs, consecutive.
    pub b: Vec<usize>,
}

impl Group {
    /// True for one original paragraph split into several revised ones.
    pub(super) fn is_split(&self) -> bool {
        self.a.len() == 1 && self.b.len() > 1
    }
}

/// What the pass found.
#[derive(Clone, Debug, Default)]
pub(super) struct Regrouped {
    /// The moves, in order of the original block.
    pub moves: Vec<Move>,
    /// The splits and the merges.
    pub groups: Vec<Group>,
}

/// One block that is in a pool.
struct Entry {
    at: Addr,
    kind: Kind,
}

/// Finds the moves, splits and merges of one story pairing.
pub(super) fn regroup(a: &StoryModel, b: &StoryModel, p: &StoryPairing) -> Regrouped {
    let mut paired_a: Vec<Vec<bool>> = Vec::new();
    let mut paired_b: Vec<Vec<bool>> = Vec::new();
    for c in &p.containers {
        let la = a.containers.get(c.ca).map_or(0, |x| x.blocks.len());
        let lb = b.containers.get(c.cb).map_or(0, |x| x.blocks.len());
        let mut va = vec![false; la];
        let mut vb = vec![false; lb];
        for pair in &c.pairs {
            if let Some(s) = va.get_mut(pair.a) {
                *s = true;
            }
            if let Some(s) = vb.get_mut(pair.b) {
                *s = true;
            }
        }
        paired_a.push(va);
        paired_b.push(vb);
    }
    let mut out = Regrouped::default();
    moves(a, b, p, &mut paired_a, &mut paired_b, &mut out);
    groups(a, b, p, &paired_a, &paired_b, &mut out);
    out
}

fn pool(m: &StoryModel, p: &StoryPairing, paired: &[Vec<bool>], side_a: bool) -> Vec<Entry> {
    let mut out = Vec::new();
    for (pi, c) in p.containers.iter().enumerate() {
        let container = if side_a { c.ca } else { c.cb };
        let Some(cont) = m.containers.get(container) else {
            continue;
        };
        let flags = paired.get(pi);
        for (i, node) in cont.blocks.iter().enumerate() {
            let is_paired = flags.and_then(|f| f.get(i)).copied().unwrap_or(true);
            if !is_paired && !node.block.is_blank() {
                out.push(Entry {
                    at: Addr {
                        pairing: pi,
                        index: i,
                    },
                    kind: node.block.kind,
                });
            }
        }
    }
    out
}

fn block<'m>(
    m: &'m StoryModel,
    p: &StoryPairing,
    at: Addr,
    side_a: bool,
) -> Option<&'m super::model::Node> {
    let c = p.containers.get(at.pairing)?;
    let container = if side_a { c.ca } else { c.cb };
    m.containers.get(container)?.blocks.get(at.index)
}

fn moves(
    a: &StoryModel,
    b: &StoryModel,
    p: &StoryPairing,
    paired_a: &mut [Vec<bool>],
    paired_b: &mut [Vec<bool>],
    out: &mut Regrouped,
) {
    let pool_a = pool(a, p, paired_a, true);
    let pool_b = pool(b, p, paired_b, false);
    let mut posting: HashMap<u64, Vec<usize>> = HashMap::new();
    for (j, e) in pool_b.iter().enumerate() {
        let Some(y) = block(b, p, e.at, false) else {
            continue;
        };
        if word_count(&y.block.norm) < MOVE_WORDS {
            continue;
        }
        let mut last = None;
        for &t in &y.block.bag {
            if last != Some(t) {
                last = Some(t);
                let list = posting.entry(t).or_default();
                if list.len() <= INDEX_CAP {
                    list.push(j);
                }
            }
        }
    }
    // (score in millionths, distance in millionths, i, j)
    let mut found: Vec<(i64, i64, usize, usize)> = Vec::new();
    let (na, nb) = (pool_a.len().max(1), pool_b.len().max(1));
    for (i, e) in pool_a.iter().enumerate() {
        let Some(x) = block(a, p, e.at, true) else {
            continue;
        };
        if word_count(&x.block.norm) < MOVE_WORDS {
            continue;
        }
        let mut cand: Vec<usize> = Vec::new();
        let mut last = None;
        for &t in &x.block.bag {
            if last != Some(t) {
                last = Some(t);
                if let Some(list) = posting.get(&t).filter(|l| l.len() <= INDEX_CAP) {
                    cand.extend(list.iter().copied());
                }
            }
        }
        cand.sort_unstable();
        cand.dedup();
        for j in cand {
            let (Some(f), Some(y)) = (
                pool_b.get(j),
                pool_b.get(j).and_then(|f| block(b, p, f.at, false)),
            ) else {
                continue;
            };
            if f.kind != e.kind {
                continue;
            }
            let score = dice(&x.block.bag, &y.block.bag);
            if score >= MOVE_FLOOR {
                let s = to_millionths(score);
                let da = to_millionths(as_f64(i) / as_f64(na));
                let db = to_millionths(as_f64(j) / as_f64(nb));
                found.push((-s, (da - db).abs(), i, j));
            }
        }
    }
    found.sort_unstable();
    let mut used_a = vec![false; pool_a.len()];
    let mut used_b = vec![false; pool_b.len()];
    let mut moves = Vec::new();
    for (_, _, i, j) in found {
        let (Some(ua), Some(ub)) = (used_a.get(i).copied(), used_b.get(j).copied()) else {
            continue;
        };
        if ua || ub {
            continue;
        }
        if let (Some(ea), Some(eb)) = (pool_a.get(i), pool_b.get(j)) {
            if let Some(s) = used_a.get_mut(i) {
                *s = true;
            }
            if let Some(s) = used_b.get_mut(j) {
                *s = true;
            }
            moves.push(Move { a: ea.at, b: eb.at });
        }
    }
    // A moved block is consumed: neither side is free for a group.
    for m in &moves {
        if let Some(s) = paired_a
            .get_mut(m.a.pairing)
            .and_then(|v| v.get_mut(m.a.index))
        {
            *s = true;
        }
        if let Some(s) = paired_b
            .get_mut(m.b.pairing)
            .and_then(|v| v.get_mut(m.b.index))
        {
            *s = true;
        }
    }
    moves.sort_by_key(|m| m.a);
    out.moves = moves;
}

fn to_millionths(x: f64) -> i64 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a value between 0 and a few units"
    )]
    let v = (x * 1_000_000.0).round() as i64;
    v
}

/// The bag of a paragraph block, if it is one, with its words counted.
fn para_bag(m: &StoryModel, container: usize, i: usize) -> Option<&[u64]> {
    let n = m.containers.get(container)?.blocks.get(i)?;
    match n.body {
        Body::Para(_) if !n.block.is_blank() => Some(&n.block.bag),
        _ => None,
    }
}

fn union(bags: &[&[u64]]) -> Vec<u64> {
    let mut all: Vec<u64> = bags.iter().flat_map(|b| b.iter().copied()).collect();
    all.sort_unstable();
    all
}

/// True if the paragraphs `window` (consecutive, on one side) cover the whole `target` bag: the union covers
/// [`GROUP_COVER`] of it, and each part after the first one that is paired adds [`GROUP_SHARE`].
fn covers(target: &[u64], parts: &[&[u64]]) -> bool {
    if target.is_empty() {
        return false;
    }
    let total = as_f64(target.len());
    let all = union(parts);
    if as_f64(common(target, &all)) / total < GROUP_COVER {
        return false;
    }
    let mut grown = Vec::new();
    let mut last = 0.0;
    for part in parts {
        if as_f64(common(target, part)) / total >= GROUP_SOLO {
            return false;
        }
        if !part.is_empty() && as_f64(common(target, part)) / as_f64(part.len()) < GROUP_PRECISION {
            return false;
        }
        grown.extend(part.iter().copied());
        grown.sort_unstable();
        let now = as_f64(common(target, &grown)) / total;
        if now - last < GROUP_SHARE {
            return false;
        }
        last = now;
    }
    true
}

fn groups(
    a: &StoryModel,
    b: &StoryModel,
    p: &StoryPairing,
    paired_a: &[Vec<bool>],
    paired_b: &[Vec<bool>],
    out: &mut Regrouped,
) {
    let mut taken_a: Vec<Vec<bool>> = paired_a.to_vec();
    let mut taken_b: Vec<Vec<bool>> = paired_b.to_vec();
    for split in [true, false] {
        for (pi, c) in p.containers.iter().enumerate() {
            for pair in &c.pairs {
                if split {
                    try_group(
                        a,
                        b,
                        pi,
                        c.ca,
                        c.cb,
                        (pair.a, pair.b),
                        true,
                        &mut taken_a,
                        &mut taken_b,
                        out,
                    );
                } else {
                    try_group(
                        a,
                        b,
                        pi,
                        c.ca,
                        c.cb,
                        (pair.a, pair.b),
                        false,
                        &mut taken_a,
                        &mut taken_b,
                        out,
                    );
                }
            }
        }
    }
    out.groups
        .sort_by(|x, y| (x.pairing, &x.a, &x.b).cmp(&(y.pairing, &y.a, &y.b)));
}

#[expect(
    clippy::too_many_arguments,
    reason = "the two models, their flags and the output are one scan"
)]
fn try_group(
    a: &StoryModel,
    b: &StoryModel,
    pi: usize,
    ca: usize,
    cb: usize,
    (ai, bi): (usize, usize),
    split: bool,
    taken_a: &mut [Vec<bool>],
    taken_b: &mut [Vec<bool>],
    out: &mut Regrouped,
) {
    // For a split the one paragraph is the original's and the run is the revised one's; a merge is the mirror.
    let (one_model, one_c, one_i, run_model, run_c, run_i, run_taken, one_taken) = if split {
        (a, ca, ai, b, cb, bi, &mut *taken_b, &mut *taken_a)
    } else {
        (b, cb, bi, a, ca, ai, &mut *taken_a, &mut *taken_b)
    };
    if one_taken.get(pi).and_then(|v| v.get(one_i)).copied() != Some(true) {
        return;
    }
    let Some(target) = para_bag(one_model, one_c, one_i) else {
        return;
    };
    if group_of(out, pi, split, one_i).is_some() {
        return;
    }
    let free = |taken: &[Vec<bool>], i: usize| {
        taken.get(pi).and_then(|v| v.get(i)).copied() == Some(false)
    };
    // windows of 2 or 3 consecutive paragraphs that hold the paired one and otherwise only free ones
    for len in 2..=GROUP_MAX {
        for start in run_i.saturating_sub(len - 1)..=run_i {
            let window: Vec<usize> = (start..start + len).collect();
            if window.iter().any(|&i| i != run_i && !free(run_taken, i)) {
                continue;
            }
            let bags: Option<Vec<&[u64]>> = window
                .iter()
                .map(|&i| para_bag(run_model, run_c, i))
                .collect();
            let Some(bags) = bags else { continue };
            if !covers(target, &bags) {
                continue;
            }
            for &i in &window {
                if let Some(s) = run_taken.get_mut(pi).and_then(|v| v.get_mut(i)) {
                    *s = true;
                }
            }
            out.groups.push(if split {
                Group {
                    pairing: pi,
                    a: vec![one_i],
                    b: window,
                }
            } else {
                Group {
                    pairing: pi,
                    a: window,
                    b: vec![one_i],
                }
            });
            return;
        }
    }
}

fn group_of(out: &Regrouped, pi: usize, split: bool, one: usize) -> Option<usize> {
    out.groups.iter().position(|g| {
        g.pairing == pi
            && if split {
                g.a.contains(&one)
            } else {
                g.b.contains(&one)
            }
    })
}
