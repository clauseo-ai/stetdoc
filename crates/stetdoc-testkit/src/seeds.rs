//! Seeds for the fuzz targets that go through `Document`, `batch` and `files`, written when `cargo xtask fuzz` asks for them.
//!
//! They are texts made with the core (a batch in block text, the readable file of a document), so they live here and not in
//! `fixtures`, which may not name the core. `fixtures::fuzz_seeds` writes the seeds of the other targets and leaves these to
//! the test below, which the same filter runs.

use crate::fixtures::contract;
use crate::fixtures::fuzz_seeds::write_seed;
use crate::laws::conformance::{CASES, FORGERIES};
use std::path::Path;
use stetdoc_core::{Batch, Document, Limits, OpenOptions};

/// What the `batch` target replaces with the document's base token (`stetdoc_core::fuzz`, behind a feature this crate does not
/// turn on, so it is written here again).
const BASE: &str = "BASE_TOKEN";

/// The seeds of the targets that go through `Document`: for `batch`, each case and forgery of the conformance suite as a batch
/// in JSON and in block text, with the placeholder for the base token; for `files`, the contract's `document.md` and lines of it.
fn through_document(target: &str) -> Vec<String> {
    match target {
        "batch" => {
            // A real token to parse with, which the seed then names by the placeholder the target replaces.
            let token = Document::open(contract::package(), &OpenOptions::default())
                .unwrap()
                .base();
            let mut out = Vec::new();
            let ops = CASES
                .iter()
                .flat_map(|c| [c.prelude, c.ops])
                .chain(FORGERIES.iter().map(|(ops, _)| *ops))
                .filter(|ops| !ops.is_empty());
            for ops in ops {
                let json = format!(r#"{{"base":"{token}","ops":[{ops}]}}"#);
                if let Ok(batch) = Batch::parse(&json, &Limits::default())
                    && let Ok(block) = batch.to_block()
                {
                    out.push(block.replace(&token, BASE));
                }
                out.push(json.replace(&token, BASE));
            }
            out
        }
        _ => {
            let doc = Document::open(contract::package(), &OpenOptions::default()).unwrap();
            let files = doc.files().unwrap();
            let md = files
                .files
                .iter()
                .find(|(name, _)| name == "document.md")
                .map(|(_, text)| text.clone())
                .unwrap_or_default();
            let mut out: Vec<String> = md.lines().map(|l| format!("{l}\n")).collect();
            out.push(md);
            out
        }
    }
}

#[test]
fn the_targets_through_the_document_have_seeds() {
    let batches = through_document("batch");
    assert!(batches.len() > 40, "{}", batches.len());
    assert!(batches.iter().all(|b| b.contains(BASE)));
    assert!(
        batches.iter().any(|b| b.starts_with("base: ")),
        "block text too"
    );
    assert!(through_document("files").len() > 5);
}

/// Writes the seeds for `batch` or `files`, as `fixtures::fuzz_seeds` does for the other targets: into the directory in
/// `STETDOC_FUZZ_SEEDS`, none longer than `STETDOC_FUZZ_MAX` bytes. Does nothing when `STETDOC_FUZZ_SEEDS` is unset or the target
/// is another.
#[test]
fn fuzz_seeds_can_be_written_for_cargo_xtask_fuzz_through_document() {
    let Ok(dir) = std::env::var("STETDOC_FUZZ_SEEDS") else {
        return;
    };
    let target =
        std::env::var("STETDOC_FUZZ_TARGET").expect("STETDOC_FUZZ_TARGET names the target");
    if target != "batch" && target != "files" {
        return;
    }
    let max: usize = std::env::var("STETDOC_FUZZ_MAX")
        .expect("STETDOC_FUZZ_MAX is the longest document")
        .parse()
        .expect("STETDOC_FUZZ_MAX is a number");
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    let written: usize = through_document(&target)
        .iter()
        .map(|t| usize::from(write_seed(dir, t.as_bytes(), max)))
        .sum();
    println!("fuzz seeds for {target}: {written} new");
    assert!(written > 0 || std::fs::read_dir(dir).unwrap().count() > 0);
}
