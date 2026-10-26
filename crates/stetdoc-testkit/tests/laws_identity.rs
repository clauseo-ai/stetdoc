//! L1 and L4 through `Document`, over the fixtures and the corpus.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use stetdoc_core::{Document, OpenOptions};
use stetdoc_testkit::fixtures;
use stetdoc_testkit::laws::identity::identity;
use stetdoc_testkit::laws::sweep::{in_parallel, over_the_corpus};

#[test]
fn every_fixture_that_opens_is_its_input_and_stable() {
    let mut opened = 0;
    let mut failures = Vec::new();
    for f in fixtures::all() {
        let bytes = f.bytes();
        if Document::open(bytes.clone(), &OpenOptions::default()).is_err() {
            continue;
        }
        opened += 1;
        if let Err(why) = identity(&bytes) {
            failures.push(format!("{}: {why}", f.name));
        }
    }
    assert!(opened > 50, "only {opened} fixtures open");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn every_curated_document_is_its_input_and_stable() {
    let Some(swept) = over_the_corpus("L1 and L4", identity) else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    swept.unwrap().verdict(200).unwrap();
}

#[test]
fn edits_on_every_fixture_that_opens_keep_their_laws() {
    use stetdoc_testkit::laws::edits::smoke;
    let mut checked = 0;
    let mut failures = Vec::new();
    for f in fixtures::all() {
        let bytes = f.bytes();
        if Document::open(bytes.clone(), &OpenOptions::default()).is_err() {
            continue;
        }
        checked += 1;
        if let Err(why) = smoke(&bytes) {
            failures.push(format!("{}: {why}", f.name));
        }
    }
    assert!(checked > 50, "only {checked} fixtures");
    // A known gap is taken out, and one that no longer occurs fails, so the entry goes when the fix does.
    for gap in stetdoc_testkit::laws::edits::KNOWN_FIXTURE_GAPS {
        let at = failures
            .iter()
            .position(|f| f.starts_with(&format!("{}: ", gap.id)) && f.contains(gap.says));
        match at {
            Some(i) => {
                failures.remove(i);
            }
            None => failures.push(format!(
                "the known gap of {} ({}) no longer occurs: remove it from KNOWN_FIXTURE_GAPS",
                gap.id, gap.says
            )),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn edits_on_the_curated_documents_keep_their_laws() {
    let Some(swept) = over_the_corpus("fixed edits", stetdoc_testkit::laws::edits::smoke) else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    let swept = swept
        .unwrap()
        .minus(stetdoc_testkit::laws::edits::KNOWN_GAPS)
        .unwrap();
    swept.verdict(200).unwrap();
}

#[test]
fn edits_of_files_on_every_fixture_that_opens_keep_their_laws() {
    use stetdoc_testkit::laws::files::smoke;
    let all: Vec<_> = fixtures::all().into_iter().collect();
    let failures = in_parallel(&all, |f| {
        let bytes = f.bytes();
        if Document::open(bytes.clone(), &OpenOptions::default()).is_err() {
            return None;
        }
        smoke(&bytes).err().map(|why| format!("{}: {why}", f.name))
    });
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn edits_of_files_on_the_curated_documents_keep_their_laws() {
    let Some(swept) = over_the_corpus("fixed edits of files", stetdoc_testkit::laws::files::smoke)
    else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    swept.unwrap().verdict(200).unwrap();
}

#[test]
fn the_read_surface_answers_on_every_fixture_that_opens() {
    use stetdoc_testkit::laws::surface::smoke;
    let all: Vec<_> = fixtures::all().into_iter().collect();
    let failures = in_parallel(&all, |f| {
        let bytes = f.bytes();
        if Document::open(bytes.clone(), &OpenOptions::default()).is_err() {
            return None;
        }
        smoke(&bytes).err().map(|why| format!("{}: {why}", f.name))
    });
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn the_read_surface_answers_on_the_curated_documents() {
    let Some(swept) = over_the_corpus("read surface", stetdoc_testkit::laws::surface::smoke) else {
        eprintln!("skipped: STETDOC_CORPUS_DIR is not set (cargo xtask check sets it)");
        return;
    };
    swept.unwrap().verdict(200).unwrap();
}
