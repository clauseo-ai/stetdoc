//! Every testkit fixture against `State::open`, as the layer table of the container's fixture test
//! (`package/corpus_laws/fixtures.rs`) lays out. That table lives in `package`, which may not depend on
//! `state`, so the row for `State::open` is here, with the same rule: a fixture opens or is refused
//! as `Fixture::seen_by_reading` says for a layer that reaches the `Document` stage and reads the
//! parts that `State::open` reads. A fixture whose expectation is wrong for `State::open` shows up
//! by name, and the fix is to its row in the testkit.

use super::tests::BODY;
use super::*;
use crate::api::error::ErrorCode;
use stetdoc_testkit::fixtures::{
    self, Stage, parts,
    zip::{Entry, Zip},
};

/// The parts `State::open` reads: the content types and the root relationships, the main part,
/// and every story part, since the refs of a document are minted for all of them. Which of the story
/// parts a particular package has is the package's business, so the names are the ones Word uses.
fn reads(part: &str) -> bool {
    let story = |prefix: &str| {
        part.strip_prefix(prefix)
            .is_some_and(|rest| rest.ends_with(".xml"))
    };
    matches!(
        part,
        "[Content_Types].xml"
            | "_rels/.rels"
            | "word/document.xml"
            | "word/footnotes.xml"
            | "word/endnotes.xml"
            | "word/comments.xml"
    ) || story("word/header")
        || story("word/footer")
}

#[test]
fn state_open_meets_every_testkit_fixture_as_it_expects() {
    let all = fixtures::all();
    assert!(all.len() > 100, "only {} fixtures", all.len());
    let mut failures = Vec::new();
    for f in &all {
        let want = f.seen_by_reading(Stage::Document, reads);
        let got = State::open(f.bytes(), &Limits::default());
        let agrees = match (want, &got) {
            (fixtures::Expect::Ok, Ok(_)) => true,
            (fixtures::Expect::Refused { code, .. }, Err(e)) => e.code().as_str() == code,
            _ => false,
        };
        if !agrees {
            let got = got.map_or_else(|e| format!("{e}"), |_| "it opened".to_owned());
            failures.push(format!("{}: expected {want}, got {got}", f.name));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} fixtures differ:\n{}",
        failures.len(),
        all.len(),
        failures.join("\n")
    );
}

#[test]
fn a_package_whose_content_types_are_not_xml_is_refused_at_open() {
    let doc = parts::Doc::new().body(BODY).bytes();
    let mut entries = parts::entries_doc(&doc);
    entries.retain(|e| e.name_bytes() != b"[Content_Types].xml");
    entries.push(Entry::new(
        "[Content_Types].xml",
        "<Types><Default Extension=\"xml\"",
    ));
    let e = State::open(Zip::new(entries).build(), &Limits::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().contains("[Content_Types].xml"), "{e}");
}

#[test]
fn a_fixture_that_opens_has_its_refs_minted_and_resumes() {
    // Everything that opens is a state with a ref table that exports and resumes: the layer table
    // checks the outcome of `open`, and this checks what it leaves.
    let mut opened = 0;
    for f in fixtures::all() {
        let bytes = f.bytes();
        let Ok(s) = State::open(bytes.clone(), &Limits::default()) else {
            continue;
        };
        opened += 1;
        let e = s
            .export()
            .unwrap_or_else(|e| panic!("{}: export: {e}", f.name));
        let rec = Recorded {
            base: e.base,
            head_sha256: e.head_sha256,
            refs_sha256: e.refs_sha256,
        };
        let back = State::resume(e.head, &e.refs, &rec, Some(&bytes), &Limits::default())
            .unwrap_or_else(|e| panic!("{}: resume: {e}", f.name));
        assert_eq!(back.base_token(), s.base_token(), "{}", f.name);
    }
    assert!(opened >= 80, "only {opened} fixtures opened");
}
