//! The constants of the cleaning rules (design `revisions.md`, 2.5) against what Word wrote. Word writes a
//! replacement as a `w:del` and a `w:ins` side by side, by one author, and the corpus has them in Word's own
//! files. The paragraphs that hold such a pair are cases: what Word did to a paragraph is a number of edits, a
//! run of deleted and inserted text with plain text on both sides of it being one, and what the cleaning of
//! 2.5 makes of the same two texts is another. Word's edits are what a person selected, so a cleaning that
//! makes more of them than Word did is choppier than a person, and one that makes fewer has merged what a
//! person kept apart. The table that this prints is the evidence for the constants. With
//! `STETDOC_CORPUS_DIR` unset the test skips.

use super::diff::{Cleaning, Cleanup, edits_with};
use super::names::is_w;
use crate::api::limits::Limits;
use crate::state::State;
use crate::wordml::{BlockKind, Mode, NodeId, Paragraph, part_revisions};
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::expect::Expect;

/// A paragraph that Word edited with a replacement.
struct Case {
    id: String,
    /// How many separate edits Word made in it.
    word_edits: usize,
    original: String,
    revised: String,
}

/// The `w:del` that has a `w:ins` of the same author right after it, as the paragraph they are in.
fn pairs(tree: &crate::xml::SpanTree) -> Vec<NodeId> {
    let mut out = Vec::new();
    for r in part_revisions(tree) {
        if r.kind != crate::wordml::RevKind::Del {
            continue;
        }
        let mut next = tree.node_ref(r.node).next_sibling();
        while let Some(n) = next {
            if !n.is_element() || super::join::MARKERS.iter().any(|m| is_w(&n, m)) {
                next = n.next_sibling();
            } else {
                break;
            }
        }
        let paired = next.is_some_and(|n| {
            crate::wordml::revision_at(&n)
                .is_some_and(|x| x.kind == crate::wordml::RevKind::Ins && x.author == r.author)
        });
        if !paired {
            continue;
        }
        let mut cur = tree.node_ref(r.node).parent();
        while let Some(a) = cur {
            if is_w(&a, "p") {
                out.push(a.id());
                break;
            }
            cur = a.parent();
        }
    }
    out
}

/// The number of separate edits in a paragraph, read from what each of its items is.
fn word_edits(p: &Paragraph<'_>) -> usize {
    let map = p.map();
    let mut edits = 0;
    let mut inside = false;
    for item in map.items().iter().filter(|i| i.len > 0) {
        let plain = item.vis == crate::wordml::Vis::BOTH;
        if !plain && !inside {
            edits += 1;
        }
        inside = !plain;
    }
    edits
}

fn gather(corpus: &Corpus) -> (usize, Vec<Case>) {
    let mut found = 0;
    let mut cases = Vec::new();
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect == Expect::Ok && e.word_fp == "y")
    {
        let Ok(state) = State::open(entry.bytes(), &Limits::default()) else {
            continue;
        };
        let view = state.parts();
        let Some(id) = view.id(super::tests_support::MAIN) else {
            continue;
        };
        let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
            continue;
        };
        let in_pairs = pairs(tree);
        found += in_pairs.len();
        let mut seen: Vec<NodeId> = Vec::new();
        for para in in_pairs {
            if seen.contains(&para) {
                continue;
            }
            seen.push(para);
            let Some(i) = story
                .blocks()
                .iter()
                .position(|e| e.kind == BlockKind::Paragraph && e.node == para)
            else {
                continue;
            };
            let Some(p) = story.paragraph(tree, i) else {
                continue;
            };
            cases.push(Case {
                id: entry.id.clone(),
                word_edits: word_edits(&p),
                original: p.text(Mode::Original),
                revised: p.text(Mode::Final),
            });
        }
    }
    (found, cases)
}

/// How many edits the cleaning makes of each case, against Word's: the cases where it makes more, and
/// by how many in all, and where it makes fewer.
fn score(cases: &[Case], cleaning: Cleaning) -> (usize, usize, usize, usize, usize) {
    let mut ours = 0;
    let (mut choppier, mut merged) = (0, 0);
    let mut excess = 0;
    for c in cases {
        let n = edits_with(&c.original, &c.revised, Cleanup::Readable, cleaning).len();
        ours += n;
        if n > c.word_edits {
            choppier += 1;
            excess += n - c.word_edits;
        } else if n < c.word_edits {
            merged += 1;
        }
    }
    (ours, choppier, merged, excess, cases.len())
}

#[test]
fn the_constants_of_the_cleaning_make_no_more_edits_than_word_did_over_its_own_replacements() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let (pairs, cases) = gather(&corpus);
    let word: usize = cases.iter().map(|c| c.word_edits).sum();
    eprintln!(
        "CALIBRATION: {pairs} pairs of a deletion and an insertion by one author in Word-written documents, in {} paragraphs of {} documents; Word made {word} edits",
        cases.len(),
        cases
            .iter()
            .map(|c| c.id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    );
    eprintln!(
        "island words, whole span : ours (edits), cases choppier than Word, cases merged more, excess edits"
    );
    for island_words in [1usize, 2, 3, 4] {
        for whole in [(1usize, 3usize), (2, 5), (1, 2), (3, 5), (2, 3)] {
            let (ours, choppier, merged, excess, _) = score(
                &cases,
                Cleaning {
                    island_words,
                    whole,
                },
            );
            let mark = if (island_words, whole)
                == (Cleaning::DEFAULT.island_words, Cleaning::DEFAULT.whole)
            {
                " <- the constants"
            } else {
                ""
            };
            eprintln!(
                "{island_words} words, {}/{} : {ours:4}, {choppier:3}, {merged:3}, {excess:4}{mark}",
                whole.0, whole.1
            );
        }
    }
    let (ours, choppier, merged, excess, n) = score(&cases, Cleaning::DEFAULT);
    eprintln!(
        "DEFAULT: {n} cases, {ours} edits against Word's {word}, {choppier} choppier, {merged} merged more, {excess} excess"
    );
    // Every combination of the table is as choppy as Word at the worst, so the data holds the constants from
    // one side only: a cleaning that makes more edits than Word did in a paragraph is choppy, and the
    // constants make none. How much to merge beyond that is a matter of readability, which the compare noise
    // figure over the LAB pairs measures later.
    assert_eq!(
        choppier, 0,
        "{choppier} of {n} paragraphs get more edits than Word made"
    );
}
