//! The speed and memory benchmarks, which exist in test builds only. They are registered here and run by
//! the harness in `stetdoc-testkit`.
//!
//! **Owns.** The list of benchmarks, the counting allocator of the test binary, and the one
//! ignored test that runs the list. A benchmark is a small file in this directory: a `Bench` that
//! names what it measures and what it is held to, and a function that prepares one run on a
//! document. The core reads no clock, no file and no environment, so none of that is here: the
//! harness in `stetdoc-testkit` (`bench`) times the run, measures its peak memory, builds the
//! inputs, holds the result to the budgets of `ARCHITECTURE.md` and to a baseline, and prints the
//! table. Nothing here is public, and a build without `cfg(test)` does not contain it.
//!
//! **Interface.** None. `cargo xtask bench` runs the ignored test `bench::g6` of this crate in a
//! release build.
//!
//! **Invariants.**
//! - The test binary counts allocations (`stats_alloc`), because the peak memory of a run is a
//!   figure the budgets are written against, and a counting global allocator is the way to get
//!   one that does not vary. It is installed here, in the crate whose test binary runs the
//!   benchmarks, and costs the other tests of the crate two atomic operations an allocation.
//! - A benchmark prepares a run without the clock and returns what the run made, so the harness
//!   drops it after the clock stops. What `prepare` builds and the run keeps counts toward the
//!   peak, as it does for a caller that holds a document; a copy of the input that the run
//!   consumes belongs in `prepare`.
//! - A benchmark is named once and for good: the baseline is keyed by the name.
//!
//! **Depends on.** `api`, `package`, `xml`, `schema`, `wordml`, `styles`, `numbering`, `state`, `revisions`,
//! `index`, `projection`, `check`, `compare`, `document`.
//!
//! **Start reading.** `bench/package.rs`, the smallest.

mod check;
mod compare;
mod document;
mod numbering;
mod package;
mod projection;
mod revisions;
mod schema;
mod state;
mod support;
mod wordml;
mod xml;

use stetdoc_testkit::bench::Bench;

#[global_allocator]
static COUNTING: &stats_alloc::StatsAlloc<std::alloc::System> = &stats_alloc::INSTRUMENTED_SYSTEM;

/// Every benchmark, in the order they run. A new one is a new file and a line here.
fn all() -> Vec<Bench> {
    vec![
        package::OPEN,
        package::SAVE,
        xml::PARSE,
        xml::SERIALIZE,
        xml::SERIALIZE_ALL_TAGS,
        xml::CLONE_EDIT,
        schema::UNITS,
        schema::PART,
        state::OPEN,
        state::WRITES,
        state::FIELD_WRITES,
        state::EDIT,
        state::SAVE,
        state::EXPORT,
        state::RESUME,
        wordml::SCAN,
        wordml::FIELD_PASS,
        wordml::TEXT_PASS,
        wordml::REFRESH,
        wordml::TEXT_EDITS,
        wordml::FIELD_EDITS,
        wordml::PARAGRAPH_INSERTS,
        numbering::BUILD,
        numbering::EDIT,
        projection::VIEW_COLD,
        projection::RANGE_NOTES_COLD,
        projection::VIEW_WARM,
        projection::RENDER_FILES,
        projection::TO_BATCH_UNEDITED,
        projection::TO_BATCH_EDIT,
        projection::LOOKUP_COLD,
        revisions::COMPILE,
        revisions::EDIT,
        revisions::ACCEPT_ALL,
        revisions::RESOLVE_IDS,
        revisions::RESOLVE_THEN_EDIT,
        revisions::CHANGES_RECEIPT,
        check::EDIT,
        check::FRAME,
        check::DOCUMENT,
        compare::CONTRACT_10,
        compare::DENSE_10,
        compare::CONTRACT_25,
        compare::DENSE_25,
        document::EDIT_OPS,
        document::EDIT_OPS_AGAIN,
        document::DRY_RUN,
        document::EDIT_FILES,
        document::VIEW_COLD,
        document::EXPORT,
        document::OPEN,
        document::SAVE,
        document::RESUME,
        document::FILES,
    ]
}

#[test]
#[ignore = "benchmarks: run them with `cargo xtask bench`, which builds for release"]
fn g6() {
    if let Err(why) = stetdoc_testkit::bench::run(&all()) {
        panic!("{why}");
    }
}

/// A row for a benchmark that was renamed or removed is never read again, and a regression of the
/// new name goes unseen until someone notices that it has no row. So the file may name only what
/// runs.
#[test]
fn the_baseline_names_only_benchmarks_and_inputs_that_exist() {
    let all = all();
    let rows = stetdoc_testkit::bench::baseline::load().unwrap_or_else(|e| panic!("{e}"));
    for row in &rows {
        let bench = all.iter().find(|b| b.name == row.bench).unwrap_or_else(|| {
            panic!(
                "baseline.tsv has a row for {}, which does not exist",
                row.bench
            )
        });
        let known = match bench.inputs {
            stetdoc_testkit::bench::Inputs::Documents => {
                stetdoc_testkit::bench::inputs::NAMES.contains(&row.input.as_str())
            }
            stetdoc_testkit::bench::Inputs::Only(only) => row.input == only,
        };
        assert!(
            known,
            "baseline.tsv: {} has no input {}",
            row.bench, row.input
        );
        assert!(
            row.platform.contains('-') && row.time_ms > 0.0,
            "baseline.tsv: a row of {} on {} is not one a run writes",
            row.bench,
            row.input
        );
    }
}

#[test]
fn the_benchmarks_have_distinct_names_and_prepare_a_run_on_a_small_document() {
    let all = all();
    let mut names: Vec<&str> = all.iter().map(|b| b.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), all.len(), "a benchmark name is used twice");
    let body = "<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/></w:pPr><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>";
    let input = stetdoc_testkit::bench::inputs::from_body("small", body);
    for bench in &all {
        let run = (bench.prepare)(&input).unwrap_or_else(|e| panic!("{}: {e}", bench.name));
        run().unwrap_or_else(|e| panic!("{}: {e}", bench.name));
    }
}
