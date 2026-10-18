//! The testkit's fixtures against the container: every fixture is opened with `Package::open` and
//! what happens has to be what its expectation says, so a fixture and the engine cannot drift apart
//! unseen. The fixtures are plain data from `stetdoc_testkit::fixtures`; the core's tests may use
//! the testkit because a type never crosses its API.
//!
//! Two tests. The first runs the container laws of `corpus_laws` on every fixture, with the
//! expectation the fixture carries as the index would give it: a document that must open is held to
//! L1 and the one-edit L2 smoke and the other reader, and one that must be refused is refused with
//! its code at its stage, not earlier, by the parts the fixture names, and every part it does not
//! name reads and parses cleanly (`Fixture::parts` says why that matters). The second compares the outcome of `open` alone with what
//! each layer must observe (`Fixture::seen_by_reading`), layer by layer. The layers above the
//! container run the same test in their own module, because `package` may not depend on them.

use super::{Expect, check_document};
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::package::Package;
use stetdoc_testkit::fixtures::{self, Fixture, Stage};

/// A fixture's expectation, as `INDEX.tsv` would give it to the container laws.
fn as_indexed(f: &Fixture) -> Expect {
    Expect {
        code: f.expect.code().to_owned(),
        stage: match f.expect {
            fixtures::Expect::Ok => None,
            fixtures::Expect::Refused { stage, .. } => Some(stage.as_str().to_owned()),
        },
        parts: f.parts.iter().map(|p| (*p).to_owned()).collect(),
    }
}

#[test]
fn the_container_laws_hold_on_every_testkit_fixture() {
    let all = fixtures::all();
    assert!(all.len() > 100, "only {} fixtures", all.len());
    // The law that the parts a fixture does not name read and parse cleanly is part of the
    // refusal check, and it is asked of every fixture whose refusal is at `read` or `xml`.
    let carrying = all
        .iter()
        .filter(|f| {
            matches!(
                f.expect,
                fixtures::Expect::Refused {
                    stage: Stage::Read | Stage::Xml,
                    ..
                }
            ) && !f.parts.is_empty()
        })
        .count();
    assert!(
        carrying >= 30,
        "only {carrying} fixtures carry a refusal at read or xml and name their parts, so the law of the unnamed parts is checked on too few"
    );
    let failures: Vec<String> = all
        .iter()
        .filter_map(|f| {
            check_document(f.bytes(), Some(&as_indexed(f)), false)
                .err()
                .map(|why| format!("{} (expects {}): {why}", f.name, f.expect))
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} fixtures break the container laws or their expectation:\n{}",
        failures.len(),
        all.len(),
        failures.join("\n")
    );
}

/// A layer of the engine whose `open` meets documents: its name, the last stage of the pipeline it
/// reaches, the parts it reads when it opens (the rest are not met until something asks for
/// them), and what its `open` does with the bytes.
struct Layer {
    name: &'static str,
    last: Stage,
    reads: fn(&str) -> bool,
    open: fn(Vec<u8>) -> Result<(), StetError>,
}

/// The layers that open a document, from the bottom. `Package::open` reads the structure of the
/// archive and no part, and reaches the `Open` stage.
///
/// **`State::open` cannot join here**, though this table was made for it: `package` may not depend
/// on `state` (the table of allowed dependencies in `ARCHITECTURE.md`, which the check enforces on
/// test code too). Its row is `state_open_meets_every_testkit_fixture_as_it_expects` in
/// `state/tests_fixtures.rs`, with the same rule: every fixture has to open or be refused in
/// `State::open` as `Fixture::seen_by_reading(Stage::Document, reads)` says. A fixture whose
/// expectation is wrong for a layer shows up by name, as `stress025.docx` did for the corpus: the
/// fix is to its row in the testkit, never to the row of the layer. A layer above `state` that
/// opens documents adds its row in its own module in the same way.
fn layers() -> Vec<Layer> {
    vec![Layer {
        name: "Package::open",
        last: Stage::Open,
        reads: |_| false,
        open: |bytes| Package::open(bytes, &Limits::default()).map(|_| ()),
    }]
}

#[test]
fn every_layer_that_opens_a_document_meets_the_fixtures_as_they_expect() {
    let all = fixtures::all();
    let mut failures = Vec::new();
    for layer in layers() {
        for f in &all {
            let want = f.seen_by_reading(layer.last, layer.reads);
            let got = (layer.open)(f.bytes());
            let agrees = match (want, &got) {
                (fixtures::Expect::Ok, Ok(())) => true,
                (fixtures::Expect::Refused { code, .. }, Err(e)) => e.code().as_str() == code,
                _ => false,
            };
            if !agrees {
                let got = got.map_or_else(|e| format!("{e}"), |()| "it opened".to_owned());
                failures.push(format!(
                    "{} on {}: expected {want}, got {got}",
                    layer.name, f.name
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} outcomes differ from the fixtures' expectations:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
