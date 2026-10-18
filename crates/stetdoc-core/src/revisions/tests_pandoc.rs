//! Pandoc as a third reader of accept and reject (design `revisions.md`, 8.2 c), where it is installed:
//! `pandoc --track-changes=accept` and `=reject` read a document with its revisions, and the words of what they
//! read are compared with the words of the main part after `accept_all` and `reject_all`. Where pandoc is not
//! installed, or the corpus is not named by `STETDOC_CORPUS_DIR`, the test skips. Pandoc writes list numbers,
//! table borders and no headers or footers, so the comparison is of the words of the body, with numbers and
//! single letters left out, and what it finds is printed for a person to read: the divergence classes of
//! pandoc are not a list that a test can hold yet.

use super::index::ChangeIndex;
use super::laws_util::runs_in;
use super::resolve::resolve;
use super::tests_support::MAIN;
use super::types::{Action, Selector, Step};
use crate::api::limits::Limits;
use crate::state::State;
use crate::wordml::{Mode, part_revisions};
use std::collections::BTreeMap;
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::expect::Expect;
use stetdoc_testkit::oracle::{TrackChanges, pandoc_text};

/// The words of a text, lower case, with numbers and single characters left out, counted.
fn words(text: &str) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for w in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() > 1 && !w.chars().all(|c| c.is_ascii_digit()))
    {
        *out.entry(w.to_lowercase()).or_default() += 1;
    }
    out
}

/// How many words one has that the other has not, counting repeats.
fn apart(a: &BTreeMap<String, usize>, b: &BTreeMap<String, usize>) -> usize {
    let mut n = 0;
    for (w, x) in a {
        n += x.saturating_sub(b.get(w).copied().unwrap_or(0));
    }
    for (w, y) in b {
        n += y.saturating_sub(a.get(w).copied().unwrap_or(0));
    }
    n
}

#[test]
fn pandoc_reads_the_words_that_accept_all_and_reject_all_leave_where_it_is_installed() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let (mut compared, mut same) = (0usize, 0usize);
    let mut differing: Vec<String> = Vec::new();
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect == Expect::Ok && e.size <= 200_000)
    {
        let Ok(state) = State::open(entry.bytes(), &Limits::default()) else {
            continue;
        };
        let view = state.parts();
        let Some(id) = view.id(MAIN) else { continue };
        let Ok(tree) = view.tree(id) else { continue };
        if part_revisions(tree).is_empty() {
            continue;
        }
        for (mode, action, side) in [
            (TrackChanges::Accept, Action::Accept, "accept"),
            (TrackChanges::Reject, Action::Reject, "reject"),
        ] {
            let Some(theirs) = pandoc_text(&entry.bytes(), mode) else {
                eprintln!("pandoc is not installed, or cannot read this: the test skips");
                return;
            };
            let mut tx = state.begin();
            let step = Step {
                action,
                selector: Selector::All,
            };
            if resolve(&mut tx, &mut ChangeIndex::new(), &[step]).is_err() {
                continue;
            }
            let ours = runs_in(&tx.parts(), MAIN, Mode::Final, true, &[]).join(" ");
            compared += 1;
            let gap = apart(&words(&ours), &words(&theirs));
            if gap == 0 {
                same += 1;
            } else {
                differing.push(format!("{} {side}: {gap} words apart", entry.id));
            }
        }
    }
    eprintln!("PANDOC: {compared} readings compared, {same} with the same words");
    for line in differing.iter().take(40) {
        eprintln!("PANDOC DIFFERS {line}");
    }
}
