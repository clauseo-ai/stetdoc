//! Tests of the pure alignment: order, determinism, kinds, anchors and the spine, the stretch program, the large
//! path, and recovery of the pairs a generator made.

use super::align::{Alignment, Block, How, Kind, align, lis};
use super::align_gap::CELLS_MAX;

/// A small deterministic generator, so that no test reads a clock or a random source.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn vocab(rng: &mut Rng, n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("w{}x{}", i, rng.below(1000)))
        .collect()
}

fn sentence(rng: &mut Rng, words: &[String], len: usize) -> String {
    (0..len)
        .map(|_| words[rng.below(words.len())].as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn paras(texts: &[String]) -> Vec<Block> {
    texts
        .iter()
        .map(|t| Block::new(Kind::Paragraph, t))
        .collect()
}

fn check_order(al: &Alignment, na: usize, nb: usize) {
    let mut last: Option<(usize, usize)> = None;
    for p in &al.pairs {
        assert!(p.a < na && p.b < nb);
        if let Some((a, b)) = last {
            assert!(p.a > a && p.b > b, "pairs must ascend in both lists");
        }
        last = Some((p.a, p.b));
    }
}

#[test]
fn identical_lists_pair_by_trimming_alone() {
    let mut rng = Rng(1);
    let w = vocab(&mut rng, 300);
    let t: Vec<String> = (0..200).map(|_| sentence(&mut rng, &w, 12)).collect();
    let al = align(&paras(&t), &paras(&t));
    assert_eq!(al.pairs.len(), 200);
    assert!(al.pairs.iter().all(|p| p.how == How::Trim && p.a == p.b));
    assert_eq!(al.stats.stretches, 0);
}

#[test]
fn empty_sides_pair_nothing() {
    let b = paras(&["a b c d e".to_owned()]);
    assert!(align(&[], &b).pairs.is_empty());
    assert!(align(&b, &[]).pairs.is_empty());
    assert!(align(&[], &[]).pairs.is_empty());
}

#[test]
fn a_table_never_pairs_with_a_paragraph_of_the_same_text() {
    let a = vec![Block::new(Kind::Table, "alpha beta gamma delta epsilon")];
    let b = vec![Block::new(
        Kind::Paragraph,
        "alpha beta gamma delta epsilon",
    )];
    assert!(align(&a, &b).pairs.is_empty());
    let a = vec![Block::new(Kind::Row, "alpha beta gamma delta epsilon")];
    let b = vec![Block::new(Kind::Table, "alpha beta gamma delta epsilon")];
    assert!(align(&a, &b).pairs.is_empty());
}

#[test]
fn quotes_spaces_and_the_non_breaking_space_do_not_stop_a_pair_but_words_do() {
    let a = paras(&["He said \u{201c}stop\u{201d}  now".to_owned()]);
    let b = paras(&["He said \"stop\"\u{a0}now".to_owned()]);
    let al = align(&a, &b);
    assert_eq!(al.pairs.len(), 1);
    assert_eq!(al.pairs[0].how, How::Trim);
}

#[test]
fn blanks_never_anchor_and_pair_by_position_inside_a_stretch() {
    let a = paras(&[
        "keep one one one one".into(),
        String::new(),
        "old text of the clause here".into(),
        String::new(),
        "keep two two two two".into(),
    ]);
    let b = paras(&[
        "keep one one one one".into(),
        String::new(),
        "new text of the clause here".into(),
        String::new(),
        "keep two two two two".into(),
    ]);
    let al = align(&a, &b);
    assert_eq!(al.pairs.len(), 5);
    assert_eq!(al.pairs.iter().filter(|p| p.how == How::Similar).count(), 1);
}

#[test]
fn the_spine_is_the_longest_increasing_run() {
    // brute force over small random inputs
    let mut rng = Rng(7);
    for _ in 0..200 {
        let n = 1 + rng.below(9);
        let mut ys: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            ys.swap(i, rng.below(i + 1));
        }
        let pairs: Vec<(usize, usize)> = ys.iter().enumerate().map(|(i, &y)| (i, y)).collect();
        let got = lis(&pairs);
        let mut best = 0;
        for mask in 0u32..(1 << n) {
            let pick: Vec<usize> = (0..n)
                .filter(|i| mask >> i & 1 == 1)
                .map(|i| ys[i])
                .collect();
            if pick.windows(2).all(|w| w[0] < w[1]) {
                best = best.max(pick.len());
            }
        }
        assert_eq!(got.len(), best);
        assert!(got.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
    }
}

/// What a generated pair knows: the revised list, and for each original block the revised index it became, if any.
struct Made {
    a: Vec<String>,
    b: Vec<String>,
    truth: Vec<Option<usize>>,
    edited: Vec<bool>,
}

/// A list of `n` unique paragraphs and a revision of it with the given share of edits, deletions, insertions and moves.
fn make(seed: u64, n: usize, pct: usize) -> Made {
    let mut rng = Rng(seed);
    let w = vocab(&mut rng, 2000);
    let a: Vec<String> = (0..n)
        .map(|i| format!("{} §{}", sentence(&mut rng, &w, 14), i))
        .collect();
    let mut b: Vec<(String, Option<usize>, bool)> = Vec::new();
    for (i, p) in a.iter().enumerate() {
        let r = rng.below(100);
        if r < pct / 2 {
            let mut ws: Vec<&str> = p.split(' ').collect();
            let k = ws.len();
            for _ in 0..2 {
                let at = rng.below(k - 1);
                ws[at] = &w[rng.below(w.len())];
            }
            b.push((ws.join(" "), Some(i), true));
        } else if r < pct * 7 / 10 {
            // deleted
        } else if r < pct * 9 / 10 {
            b.push((p.clone(), Some(i), false));
            let fresh = format!("{} §n{}", sentence(&mut rng, &w, 16), rng.below(1_000_000));
            b.push((fresh, None, false));
        } else {
            b.push((p.clone(), Some(i), false));
        }
    }
    let mut truth = vec![None; n];
    let mut edited = vec![false; n];
    for (j, (_, from, ed)) in b.iter().enumerate() {
        if let Some(i) = from {
            truth[*i] = Some(j);
            edited[*i] = *ed;
        }
    }
    Made {
        a,
        b: b.into_iter().map(|(t, _, _)| t).collect(),
        truth,
        edited,
    }
}

fn recovery(m: &Made, al: &Alignment) -> (usize, usize) {
    let mut right = 0;
    let mut wrong = 0;
    let mut got = vec![None; m.a.len()];
    for p in &al.pairs {
        got[p.a] = Some(p.b);
    }
    for (i, t) in m.truth.iter().enumerate() {
        match (t, got[i]) {
            (Some(t), Some(g)) if *t == g => right += 1,
            (_, Some(_)) => wrong += 1,
            _ => {}
        }
    }
    (right, wrong)
}

#[test]
fn the_pairs_a_generator_made_are_recovered() {
    for (seed, n, pct) in [(1, 400, 5), (2, 400, 25), (3, 1200, 10), (4, 300, 60)] {
        let m = make(seed, n, pct);
        let al = align(&paras(&m.a), &paras(&m.b));
        check_order(&al, m.a.len(), m.b.len());
        let (right, wrong) = recovery(&m, &al);
        let truth_pairs = m.truth.iter().filter(|t| t.is_some()).count();
        assert_eq!(
            wrong, 0,
            "seed {seed}: a block was paired with the wrong partner"
        );
        assert!(
            right * 1000 >= truth_pairs * 990,
            "seed {seed}: {right} of {truth_pairs} pairs recovered"
        );
        let _ = &m.edited;
    }
}

#[test]
fn alignment_is_deterministic() {
    let m = make(9, 500, 25);
    let (a, b) = (paras(&m.a), paras(&m.b));
    let x = align(&a, &b);
    let y = align(&a, &b);
    assert_eq!(x.pairs, y.pairs);
    assert_eq!(x.stats, y.stats);
}

#[test]
fn a_large_stretch_goes_through_the_index_and_stays_bounded() {
    // 700 against 700 blocks, every one edited so that no key is shared: no anchor, 490,000 cells.
    let mut rng = Rng(11);
    let w = vocab(&mut rng, 3000);
    let base: Vec<String> = (0..700)
        .map(|i| format!("{} §{}", sentence(&mut rng, &w, 20), i))
        .collect();
    let revised: Vec<String> = base
        .iter()
        .map(|p| {
            let mut ws: Vec<&str> = p.split(' ').collect();
            let k = ws.len();
            ws[rng.below(k - 1)] = "changed";
            ws.join(" ")
        })
        .collect();
    let (a, b) = (paras(&base), paras(&revised));
    assert!(a.len() * b.len() > CELLS_MAX);
    let al = align(&a, &b);
    assert_eq!(al.stats.large, 1);
    assert!(al.pairs.len() >= 690, "only {} pairs", al.pairs.len());
    assert!(al.pairs.iter().all(|p| p.a == p.b));
    assert!(al.stats.scored < 700 * 100, "scored {}", al.stats.scored);
}

#[test]
fn two_unrelated_lists_are_bounded() {
    let mut rng = Rng(21);
    let w = vocab(&mut rng, 150);
    let a: Vec<String> = (0..1500).map(|_| sentence(&mut rng, &w, 15)).collect();
    let b: Vec<String> = (0..1500).map(|_| sentence(&mut rng, &w, 15)).collect();
    let al = align(&paras(&a), &paras(&b));
    check_order(&al, 1500, 1500);
    assert!(al.stats.large >= 1);
    // the index cap keeps the work linear: nowhere near 1500 * 1500
    assert!(al.stats.scored < 1500 * 200, "scored {}", al.stats.scored);
}

#[test]
fn a_shuffled_list_keeps_a_spine_and_leaves_the_rest_unpaired() {
    let mut rng = Rng(31);
    let w = vocab(&mut rng, 2000);
    let a: Vec<String> = (0..300)
        .map(|i| format!("{} §{}", sentence(&mut rng, &w, 12), i))
        .collect();
    let mut b = a.clone();
    for i in (1..b.len()).rev() {
        b.swap(i, rng.below(i + 1));
    }
    let al = align(&paras(&a), &paras(&b));
    check_order(&al, 300, 300);
    for p in &al.pairs {
        assert_eq!(a[p.a], b[p.b]);
    }
    assert!(al.pairs.len() >= 20 && al.pairs.len() < 300);
}
