//! Seeds for the fuzz targets, written when `cargo xtask fuzz` asks for them.
//!
//! The seeds are the fixtures and, from the fetched curated corpus, the documents and XML parts
//! that are small enough. The `package` target takes whole documents and the `xml` target takes
//! XML parts. Each seed is four bytes of choices followed by the document, as the targets read
//! it. The choices are the first four bytes of the document's SHA-256, so the seeds do not all
//! pick the same edit, and a fuzzer changes them freely from there.

use super::inspect;
use super::*;
use crate::corpus::Corpus;
use sha2::{Digest, Sha256};
use std::path::Path;

/// The XML parts of a document that are at most `max` bytes. A document that is not a ZIP, or has
/// a part that does not read, gives what it can.
fn xml_parts(zip: &[u8], max: usize) -> Vec<Vec<u8>> {
    let Ok(archive) = inspect::read(zip) else {
        return Vec::new();
    };
    archive
        .records
        .iter()
        .filter(|r| {
            let name = String::from_utf8_lossy(&r.name).to_ascii_lowercase();
            (name.ends_with(".xml") || name.ends_with(".rels")) && r.usize <= max as u64
        })
        .filter_map(|r| inspect::extract(zip, r).ok())
        .collect()
}

/// Writes `document` under `dir`, named by the hash of the seed, unless it is too large or is
/// there already. Returns whether it wrote a file.
pub(crate) fn write_seed(dir: &Path, document: &[u8], max: usize) -> bool {
    if document.len() > max {
        return false;
    }
    let hash = Sha256::digest(document);
    let mut seed = hash.get(..4).unwrap_or(&[0; 4]).to_vec();
    seed.extend(document);
    let name: String = Sha256::digest(&seed)
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect();
    let path = dir.join(name);
    if path.exists() {
        return false;
    }
    std::fs::write(path, seed).unwrap();
    true
}

/// Writes the seeds for the target in `STETDOC_FUZZ_TARGET` (`package` or `xml`) into the
/// directory in `STETDOC_FUZZ_SEEDS`, none longer than `STETDOC_FUZZ_MAX` bytes. Does nothing
/// when `STETDOC_FUZZ_SEEDS` is unset, so the ordinary test run skips it.
#[test]
fn fuzz_seeds_can_be_written_for_cargo_xtask_fuzz() {
    let Ok(dir) = std::env::var("STETDOC_FUZZ_SEEDS") else {
        return;
    };
    let target =
        std::env::var("STETDOC_FUZZ_TARGET").expect("STETDOC_FUZZ_TARGET names the target");
    let max: usize = std::env::var("STETDOC_FUZZ_MAX")
        .expect("STETDOC_FUZZ_MAX is the longest document")
        .parse()
        .expect("STETDOC_FUZZ_MAX is a number");
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).unwrap();
    // The targets that go through `Document` take texts: the batches of the conformance suite, and the readable file of the
    // contract, which their seeds are, and nothing from the corpus.
    // The targets that go through `Document` take texts, which `crate::seeds` writes: this module may not name the core.
    if target == "batch" || target == "files" {
        return;
    }
    let documents = |bytes: Vec<u8>| -> Vec<Vec<u8>> {
        match target.as_str() {
            "package" => vec![bytes],
            "xml" => xml_parts(&bytes, max),
            other => panic!("no fuzz target named {other}"),
        }
    };
    // The `package` target takes the document itself, so a larger one is never a seed. The `xml`
    // target takes parts of a document, which is compressed, and it is read if the parts of it
    // can fit.
    let largest_document = match target.as_str() {
        "package" => max as u64,
        _ => 64 * max as u64,
    };
    let (mut from_fixtures, mut from_corpus) = (0, 0);
    for f in all() {
        for d in documents(f.bytes()) {
            from_fixtures += usize::from(write_seed(dir, &d, max));
        }
    }
    if let Some(corpus) = Corpus::from_env() {
        for e in corpus.unwrap().entries {
            if e.size > largest_document {
                continue;
            }
            for d in documents(e.bytes()) {
                from_corpus += usize::from(write_seed(dir, &d, max));
            }
        }
    }
    println!(
        "fuzz seeds for {target}: {from_fixtures} new from the fixtures, {from_corpus} new from the corpus"
    );
    assert!(from_fixtures + from_corpus > 0 || std::fs::read_dir(dir).unwrap().count() > 0);
}

#[test]
fn a_seed_is_four_bytes_of_choices_then_the_document() {
    let dir = tempfile::tempdir().unwrap();
    assert!(write_seed(dir.path(), b"<a/>", 100));
    assert!(
        !write_seed(dir.path(), b"<a/>", 100),
        "the same seed is written once"
    );
    assert!(
        !write_seed(dir.path(), b"<a/>", 3),
        "a document over the limit is left out"
    );
    let file = std::fs::read_dir(dir.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let seed = std::fs::read(file).unwrap();
    assert_eq!(seed.get(4..), Some(&b"<a/>"[..]));
    assert_eq!(seed.get(..4), Sha256::digest(b"<a/>").get(..4));
}

#[test]
fn the_xml_parts_of_a_fixture_are_found_and_the_bombs_are_not_inflated() {
    let plain = find_fixture("zip-stored-all").bytes();
    let parts = xml_parts(&plain, 100_000);
    assert!(parts.len() >= 4, "{}", parts.len());
    assert!(parts.iter().any(|p| p.starts_with(b"<?xml")));
    let bomb = find_fixture("zip-bomb-1000mib-declared").bytes();
    let parts = xml_parts(&bomb, 100_000);
    assert_eq!(
        parts.len(),
        5,
        "the five XML parts, and not the 1000 MiB one"
    );
    assert!(parts.iter().all(|p| p.len() <= 100_000));
    assert!(xml_parts(b"not a zip", 100).is_empty());
}

fn find_fixture(name: &str) -> Fixture {
    all().into_iter().find(|f| f.name == name).unwrap()
}
