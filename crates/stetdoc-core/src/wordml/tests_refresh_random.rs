//! A seeded random sequence of edits over a body with fields, each one refreshed from the one before
//! and compared with a scan from scratch (see `tests_refresh.rs` for the fixed cases).

use super::tests_refresh::{Doc, find, kids, toc};
use super::*;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// One random edit of the body. Returns the blocks that were changed in place, which is what `touched`
/// is for: insertions, removals and moves are left for the refresh to find.
fn step(doc: &mut Doc, rng: &mut Rng) -> Vec<NodeId> {
    let body = doc.body();
    let blocks = doc.blocks();
    let n = blocks.len();
    let pick = |rng: &mut Rng| blocks[rng.below(n)];
    match rng.below(9) {
        0 => {
            let p = doc.new_paragraph(&["new"]);
            let at = rng.below(n + 1);
            doc.insert_at(body, at, p);
            Vec::new()
        }
        1 if n > 2 => {
            let p = pick(rng);
            doc.detach(p);
            Vec::new()
        }
        2 => {
            let p = pick(rng);
            doc.detach(p);
            let at = rng.below(n);
            doc.insert_at(body, at, p);
            Vec::new()
        }
        3 => {
            let p = pick(rng);
            if find(&doc.tree, p, "t").is_some() {
                doc.set_text_of(p, "edited");
            }
            vec![p]
        }
        4 => {
            let p = pick(rng);
            let kind = ["begin", "separate", "end"][rng.below(3)];
            doc.field_run(p, kind);
            vec![p]
        }
        5 => {
            let p = pick(rng);
            if let Some(&run) = kids(&doc.tree, p).get(rng.below(3)) {
                doc.detach(run);
            }
            vec![p]
        }
        6 => {
            // split after the first run
            let p = pick(rng);
            let runs = kids(&doc.tree, p);
            if runs.len() < 2 {
                return Vec::new();
            }
            let at = 1 + rng.below(runs.len() - 1);
            let new = doc.new_paragraph(&[]);
            for r in runs.iter().skip(at) {
                doc.detach(*r);
                doc.tree.edit().append_child(new, *r).unwrap();
            }
            let index = kids(&doc.tree, body).iter().position(|&k| k == p).unwrap();
            doc.insert_at(body, index + 1, new);
            vec![p]
        }
        7 if n > 1 => {
            // merge a paragraph into the one before it
            let i = 1 + rng.below(n - 1);
            let (a, b) = (blocks[i - 1], blocks[i]);
            for r in kids(&doc.tree, b) {
                doc.detach(r);
                doc.tree.edit().append_child(a, r).unwrap();
            }
            doc.detach(b);
            vec![a]
        }
        _ => {
            let p = doc.new_paragraph(&[]);
            let kind = ["begin", "end"][rng.below(2)];
            doc.field_run(p, kind);
            let at = rng.below(n + 1);
            doc.insert_at(body, at, p);
            Vec::new()
        }
    }
}

#[test]
fn a_hundred_random_sequences_of_edits_each_refresh_equal_to_a_scan() {
    let (mut carried_count, mut rebuilt_count) = (0, 0);
    for seed in 1..=100u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut doc = toc();
        for round in 0..30 {
            if rng.below(3) != 0 {
                doc.prime();
            }
            let touched = step(&mut doc, &mut rng);
            let (_, carried) = doc.refresh(&touched);
            if doc.story.blocks().len() > 1 {
                if carried {
                    carried_count += 1;
                } else {
                    rebuilt_count += 1;
                }
            }
            let _ = round;
        }
    }
    assert!(
        carried_count > 200,
        "the fast path ran {carried_count} times"
    );
    assert!(rebuilt_count > 200, "the rebuild ran {rebuilt_count} times");
}

#[test]
fn a_refresh_that_changes_the_story_keeps_the_part_and_kind() {
    let mut doc = toc();
    let p = doc.new_paragraph(&["x"]);
    doc.insert_at(doc.body(), 0, p);
    let (r, _) = doc.refresh(&[]);
    assert_eq!(r.story.part(), &doc.name);
    assert_eq!(r.story.kind(), StoryKind::Body);
}
