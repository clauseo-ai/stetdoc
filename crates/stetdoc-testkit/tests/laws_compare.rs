//! The laws of a comparison through `Document`: N5 and N7 over pairs of the contract fixture, over the compare pairs of the corpus
//! (`corpus/pairs.tsv`, resolved by `pairs`), and over pairs made from the curated set: each document against an edited form of
//! itself, a shape the manifest does not have.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::atomic::{AtomicUsize, Ordering};
use stetdoc_core::{Document, OpenOptions};
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::fixtures::contract;
use stetdoc_testkit::laws::compare::{Verdict, edited, validity};
use stetdoc_testkit::laws::sweep::in_parallel;

/// The documents small enough to compare many times in a debug build.
const MAX_BYTES: usize = 120_000;

#[test]
fn the_contract_and_its_edited_form_hold_n5() {
    let a = contract::package();
    let b = edited(&a).expect("the contract takes the edits");
    assert!(validity(&a, &b).unwrap().compared);
    assert!(validity(&b, &a).unwrap().compared);
    assert!(validity(&a, &a).unwrap().compared);
    let rich = contract::package_rich();
    assert!(validity(&a, &rich).unwrap().compared);
    assert!(validity(&rich, &a).unwrap().compared);
}

/// What a run over many pairs found.
#[derive(Default)]
struct Tally {
    compared: AtomicUsize,
    merged_pairs: AtomicUsize,
    merged_blocks: AtomicUsize,
    excluded_pairs: AtomicUsize,
}

impl Tally {
    fn add(&self, v: Verdict) {
        if v.compared {
            self.compared.fetch_add(1, Ordering::Relaxed);
        }
        if v.merged > 0 {
            self.merged_pairs.fetch_add(1, Ordering::Relaxed);
            self.merged_blocks.fetch_add(v.merged, Ordering::Relaxed);
        }
        if v.excluded > 0 {
            self.excluded_pairs.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn say(&self, what: &str, pairs: usize, failures: usize) -> usize {
        let compared = self.compared.load(Ordering::Relaxed);
        eprintln!(
            "{what}: {pairs} pairs, {compared} compared, {} not compared, {failures} failed N5 or N7; compare_keys_merged on {} pairs ({} blocks); exclusions on {} pairs",
            pairs - compared,
            self.merged_pairs.load(Ordering::Relaxed),
            self.merged_blocks.load(Ordering::Relaxed),
            self.excluded_pairs.load(Ordering::Relaxed),
        );
        compared
    }
}

#[test]
fn the_compare_pairs_of_the_corpus_hold_n5_and_n7() {
    let Some(found) = stetdoc_testkit::pairs::from_env() else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    let (scope, pairs) = found.unwrap_or_else(|e| panic!("the compare pairs: {e}"));
    let tally = Tally::default();
    let failures = in_parallel(&pairs, |r| {
        match validity(&r.original.bytes(), &r.revised.bytes()) {
            Ok(v) => {
                tally.add(v);
                None
            }
            Err(why) => Some(format!("pairs.tsv:{} {}: {why}", r.pair.line, r.pair.label)),
        }
    });
    let what = format!("the {} compare pairs of the corpus", scope.name());
    let compared = tally.say(&what, pairs.len(), failures.len());
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        !pairs.is_empty() && compared == pairs.len(),
        "{compared} of {} pairs were compared",
        pairs.len()
    );
}

#[test]
fn each_curated_document_against_its_edited_form_holds_n5() {
    let Some(corpus) = Corpus::from_env() else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    let corpus = corpus.unwrap();
    let pairs: Vec<(String, Vec<u8>, Vec<u8>)> = corpus
        .expected_ok()
        .filter_map(|e| {
            let bytes = e.bytes();
            if bytes.len() > MAX_BYTES
                || Document::open(bytes.clone(), &OpenOptions::default()).is_err()
            {
                return None;
            }
            edited(&bytes).map(|revised| (e.id.clone(), bytes, revised))
        })
        .take(40)
        .collect();
    let tally = Tally::default();
    let failures = in_parallel(&pairs, |(id, a, b)| match validity(a, b) {
        Ok(v) => {
            tally.add(v);
            None
        }
        Err(why) => Some(format!("{id} against its edited form: {why}")),
    });
    let compared = tally.say(
        "curated documents against their edited forms",
        pairs.len(),
        failures.len(),
    );
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(compared >= 30, "only {compared} pairs were compared");
}
