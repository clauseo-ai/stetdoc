//! The curated corpus, as `cargo xtask corpus fetch` left it: every file is there, is the
//! size and hash the manifest says, and every tier is represented.
//!
//! `cargo xtask check` runs `corpus fetch` and sets `STETDOC_CORPUS_DIR` first. Outside it,
//! the test skips when the variable is unset.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use sha2::{Digest, Sha256};
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::expect::{Expect, Stage};

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn every_curated_file_matches_its_manifest_entry() {
    let Some(corpus) = Corpus::from_env() else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    let corpus = corpus.unwrap();
    assert!(
        corpus.entries.len() >= 250,
        "only {} documents",
        corpus.entries.len()
    );
    for e in &corpus.entries {
        let bytes = e.bytes();
        assert_eq!(bytes.len() as u64, e.size, "{}", e.id);
        assert_eq!(hex(&bytes), e.sha256, "{}", e.id);
    }
    for tier in [
        "lab",
        "word-authored",
        "other-producers",
        "public",
        "edge",
        "hostile",
    ] {
        assert!(corpus.tier(tier).count() > 0, "no {tier} documents");
    }
}

#[test]
fn refusals_are_only_in_the_hostile_tier_and_say_why() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap();
    for e in corpus.expected_refused() {
        assert_eq!(e.tier, "hostile", "{}", e.id);
        assert!(matches!(e.expect, Expect::Refused { .. }), "{}", e.id);
        assert_ne!(e.reason, "-", "{} is refused without a reason", e.id);
    }
    assert!(corpus.expected_refused().count() > 0);
    for e in corpus.tier("hostile") {
        assert_ne!(e.reason, "-", "{} is hostile without a reason", e.id);
    }
}

#[test]
fn a_read_or_xml_refusal_names_the_parts_that_carry_it_and_no_other_row_does() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap();
    for e in &corpus.entries {
        let carried = matches!(
            e.expect,
            Expect::Refused {
                stage: Stage::Read | Stage::Xml,
                ..
            }
        );
        assert_eq!(!e.parts.is_empty(), carried, "{}", e.id);
        assert!(e.parts.iter().all(|p| !p.starts_with('/')), "{}", e.id);
        if carried {
            assert!(
                e.seen_by_reading(Stage::Document, |_| false) == Expect::Ok,
                "{}: a layer that reads none of its parts must see Ok",
                e.id
            );
        }
    }
}
