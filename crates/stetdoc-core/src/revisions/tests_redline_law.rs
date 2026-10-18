//! The two compare laws for the compare primitives on random pairs of formatted paragraphs: 300 pairs, each written as a redline
//! with `redline_paragraph` and checked by accepting and rejecting everything.

use super::redline_types::{ParaRedline, RedlineEdit, RunSnapshot};
use super::tests_redline::{check, detached_run};
use super::tests_support::*;

/// A deterministic generator for the law test, so that nothing reads a clock.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }
}

type Runs = Vec<(String, bool)>;

fn runs_xml(runs: &Runs) -> String {
    let body: String = runs
        .iter()
        .map(|(t, bold)| {
            let rpr = if *bold { "<w:rPr><w:b/></w:rPr>" } else { "" };
            format!("<w:r>{rpr}<w:t xml:space=\"preserve\">{t}</w:t></w:r>")
        })
        .collect();
    p(&body)
}

fn random_runs(rng: &mut Lcg, vocab: &[&str], words: usize) -> Runs {
    let mut out: Runs = Vec::new();
    let mut bold = false;
    let mut cur = String::new();
    for i in 0..words {
        if rng.next().is_multiple_of(5) && !cur.is_empty() {
            out.push((std::mem::take(&mut cur), bold));
            bold = !bold;
        }
        if i > 0 {
            cur.push(' ');
        }
        cur.push_str(vocab[rng.next() % vocab.len()]);
    }
    if !cur.is_empty() {
        out.push((cur, bold));
    }
    out
}

fn text_of(runs: &Runs) -> String {
    runs.iter().map(|(t, _)| t.as_str()).collect()
}

/// The runs that hold the characters `range` of `runs`, cut at its ends, each with its own formatting.
fn slice_runs(runs: &Runs, range: std::ops::Range<usize>) -> Runs {
    let mut out = Runs::new();
    let mut at = 0usize;
    for (t, bold) in runs {
        let n = t.chars().count();
        let (s, e) = (range.start.max(at), range.end.min(at + n));
        if s < e {
            out.push((t.chars().skip(s - at).take(e - s).collect(), *bold));
        }
        at += n;
    }
    out
}

#[test]
fn the_two_laws_hold_for_random_pairs_of_formatted_paragraphs() {
    use super::diff::{Cleanup, edits};
    let vocab = [
        "alpha", "beta", "gamma", "delta", "the", "of", "and", "clause", "notice", "party",
    ];
    let mut rng = Lcg(2026);
    let mut with_edits = 0;
    for case in 0..300 {
        let words = 3 + rng.next() % 14;
        let original = random_runs(&mut rng, &vocab, words);
        let mut revised_words = text_of(&original);
        // the revised text: a random mutation of the original's words, so that the pair has edits of every shape
        let mut ws: Vec<String> = revised_words.split(' ').map(str::to_owned).collect();
        for _ in 0..(1 + rng.next() % 3) {
            match rng.next() % 3 {
                0 if ws.len() > 1 => {
                    ws.remove(rng.next() % ws.len());
                }
                1 => ws.insert(
                    rng.next() % (ws.len() + 1),
                    vocab[rng.next() % vocab.len()].to_owned(),
                ),
                _ => {
                    let k = rng.next() % ws.len();
                    ws[k] = vocab[rng.next() % vocab.len()].to_owned();
                }
            }
        }
        revised_words = ws.join(" ");
        let revised: Runs = if rng.next().is_multiple_of(2) {
            vec![(revised_words.clone(), false)]
        } else {
            let cut = revised_words.chars().count() / 2;
            vec![
                (revised_words.chars().take(cut).collect(), false),
                (revised_words.chars().skip(cut).collect(), true),
            ]
        };
        let (old, new) = (text_of(&original), text_of(&revised));
        let es = edits(&old, &new, Cleanup::Readable);
        if es.is_empty() {
            continue;
        }
        with_edits += 1;
        // the plan, as compare makes it: the revised range of each edit and the original's runs of its range
        let mut delta: isize = 0;
        let mut planned: Vec<(std::ops::Range<usize>, Runs)> = Vec::new();
        for e in &es {
            let start = (e.range.start as isize + delta) as usize;
            planned.push((
                start..start + e.with.chars().count(),
                slice_runs(&original, e.range.clone()),
            ));
            delta += e.with.chars().count() as isize - e.range.len() as isize;
        }
        // the formatting that differs over the text that no edit touches
        let bold_a: Vec<bool> = original
            .iter()
            .flat_map(|(t, b)| t.chars().map(move |_| *b))
            .collect();
        let bold_b: Vec<bool> = revised
            .iter()
            .flat_map(|(t, b)| t.chars().map(move |_| *b))
            .collect();
        let mut fmt: Vec<RunSnapshot> = Vec::new();
        {
            let mut a_at = 0usize;
            let mut b_at = 0usize;
            let mut stretches: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> = Vec::new();
            for e in &es {
                let len_b = e.with.chars().count();
                stretches.push((a_at..e.range.start, b_at..b_at + (e.range.start - a_at)));
                b_at += (e.range.start - a_at) + len_b;
                a_at = e.range.end;
            }
            stretches.push((a_at..bold_a.len(), b_at..b_at + (bold_a.len() - a_at)));
            for (ra, rb) in stretches {
                let first_of_stretch = fmt.len();
                for (x, y) in ra.zip(rb) {
                    if bold_a[x] != bold_b[y] {
                        let in_stretch = fmt.len() > first_of_stretch;
                        match fmt.last_mut() {
                            Some(last)
                                if in_stretch
                                    && last.range.end == y
                                    && (last.snapshot.is_empty() != bold_a[x]) =>
                            {
                                last.range.end = y + 1
                            }
                            _ => fmt.push(RunSnapshot {
                                range: y..y + 1,
                                snapshot: if bold_a[x] {
                                    vec![pel("b", &[], Vec::new())]
                                } else {
                                    Vec::new()
                                },
                            }),
                        }
                    }
                }
            }
        }
        eprintln!(
            "case {case}\noriginal {original:?}\nrevised {revised:?}\nplanned {planned:?}\nfmt {fmt:?}"
        );
        let (_, xml) = check(
            &runs_xml(&original),
            &runs_xml(&revised),
            |tx, _| {
                planned
                    .iter()
                    .flat_map(|(_, fragment)| {
                        fragment
                            .iter()
                            .map(|(t, b)| detached_run(tx, t, *b))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            },
            |c, all, nodes| {
                let mut at = 0;
                let mut edits_out = Vec::new();
                for (range, fragment) in &planned {
                    edits_out.push(RedlineEdit {
                        insert: range.clone(),
                        deleted: nodes[at..at + fragment.len()].to_vec(),
                    });
                    at += fragment.len();
                }
                let plan = ParaRedline {
                    edits: edits_out,
                    fmt: fmt.clone(),
                    ..ParaRedline::default()
                };
                vec![c.redline_paragraph(&all[0], &plan).unwrap_or_else(|e| {
                    panic!(
                        "{e}: case {case} plan {plan:?}\noriginal {original:?}\nrevised {revised:?}"
                    )
                })]
            },
        );
        let _ = (case, xml);
    }
    assert!(with_edits > 200, "only {with_edits} cases had edits");
}
