//! Tests of what the fixtures promise: every expectation is allowed, every hostile fixture says
//! why, the bytes of every fixture are pinned by a snapshot, and the fixtures can be dumped.

use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// The reason vocabulary the manifest uses, which hostile fixtures draw from too.
const REASONS: &str = include_str!("../../../../corpus/reasons.tsv");

fn vocabulary() -> BTreeSet<&'static str> {
    REASONS
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| l.split('\t').next())
        .collect()
}

fn find(name: &str) -> Fixture {
    all()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
}

#[test]
fn every_edge_and_word_fixture_must_open() {
    for f in edge().into_iter().chain(word()) {
        assert_eq!(
            f.expect,
            Expect::Ok,
            "{} is not hostile, so it must open",
            f.name
        );
        assert_eq!(f.reason, "", "{}", f.name);
    }
}

#[test]
fn the_constructor_adds_no_default_part_so_every_fixture_must_name_its_own() {
    let bare = hostile_fixture(
        "bare",
        "a read refusal that names no part",
        Expect::read("malformed_input"),
        "zip.crc_mismatch",
        Vec::new,
    );
    assert!(
        bare.parts.is_empty(),
        "a default part would hide a fixture that forgot to name where its refusal sits"
    );
    let named = bare.in_parts(DOCUMENT_XML);
    assert_eq!(named.parts, ["word/document.xml"]);
}

#[test]
fn a_read_or_xml_refusal_names_its_parts_and_no_other_fixture_has_any() {
    for f in all() {
        let carried = matches!(
            f.expect,
            Expect::Refused {
                stage: Stage::Read | Stage::Xml,
                ..
            }
        );
        assert_eq!(!f.parts.is_empty(), carried, "{}", f.name);
        assert!(
            f.parts.iter().all(|p| !p.is_empty() && !p.starts_with('/')),
            "{}",
            f.name
        );
    }
    assert_eq!(
        find("opc-content-types-malformed").parts,
        ["[Content_Types].xml"]
    );
}

#[test]
fn every_hostile_fixture_says_why_in_the_census_vocabulary() {
    let vocabulary = vocabulary();
    for f in hostile() {
        assert!(!f.reason.is_empty(), "{} has no reason", f.name);
        for token in f.reason.split(';') {
            assert!(
                vocabulary.contains(token),
                "{}: `{token}` is not in corpus/reasons.tsv",
                f.name
            );
        }
    }
}

#[test]
fn the_hostile_fixtures_that_must_open_are_the_three_that_are_survived() {
    let survived: BTreeSet<&str> = hostile()
        .iter()
        .filter(|f| f.expect.is_ok())
        .map(|f| f.name)
        .collect();
    assert_eq!(
        survived,
        BTreeSet::from([
            "opc-rel-target-traversal",
            "opc-zip-in-zip-embedded",
            "xml-comment-double-hyphen"
        ]),
        "a hostile fixture that opens needs a reason to be judged ok at the engine level"
    );
}

#[test]
fn refusals_are_judged_at_the_engine_level_not_the_package_layer() {
    let malformed = "malformed_input";
    for name in [
        "opc-missing-content-types",
        "opc-missing-root-rels",
        "opc-main-part-missing",
        "zip-eocd-only",
    ] {
        assert_eq!(find(name).expect, Expect::document(malformed), "{name}");
    }
    assert_eq!(
        find("xml-nul-and-control-chars").expect,
        Expect::xml(malformed)
    );
    assert_eq!(find("xml-xml11-declaration").expect, Expect::xml(malformed));
    assert_eq!(
        find("zip-local-method-differs").expect,
        Expect::open(malformed)
    );
    assert_eq!(
        find("zip-duplicate-case-names-valid-parts").expect,
        Expect::open(malformed)
    );
    assert_eq!(
        find("xml-doctype-internal-subset-plain").expect,
        Expect::xml(malformed)
    );
    assert_eq!(
        find("zip-ole-cfb-header").expect,
        Expect::open("unsupported")
    );
    assert_eq!(
        find("zip-bomb-ratio-1000").expect,
        Expect::read("limit_exceeded")
    );
    assert_eq!(
        find("xml-deep-nesting-20000").expect,
        Expect::xml("limit_exceeded")
    );
}

#[test]
fn every_error_code_and_every_stage_is_used() {
    let refusals: Vec<(&str, Stage)> = hostile()
        .iter()
        .filter_map(|f| match f.expect {
            Expect::Refused { code, stage } => Some((code, stage)),
            Expect::Ok => None,
        })
        .collect();
    let codes: BTreeSet<&str> = refusals.iter().map(|(c, _)| *c).collect();
    assert_eq!(
        codes,
        BTreeSet::from(["malformed_input", "limit_exceeded", "unsupported"])
    );
    let stages: BTreeSet<Stage> = refusals.iter().map(|(_, s)| *s).collect();
    assert_eq!(stages, BTreeSet::from(Stage::ALL));
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A table of every fixture: name, size and SHA-256 of its bytes.
fn hash_table() -> String {
    let mut table = String::new();
    for f in all() {
        let bytes = f.bytes();
        table.push_str(&format!(
            "{}  {:>9}  {}\n",
            sha256(&bytes),
            bytes.len(),
            f.name
        ));
    }
    table
}

/// The bytes of every fixture are pinned. A change to the builder, to `flate2` or to the
/// deflate backend under it shows up here as a reviewed diff, never silently: an edit that
/// changes a fixture changes what the tests prove.
#[test]
fn the_bytes_of_every_fixture_are_pinned_by_a_snapshot() {
    insta::assert_snapshot!("fixture_hashes", hash_table());
}

/// Writes every fixture to the directory in `STETDOC_DUMP_FIXTURES`, with a description
/// file, so another tool can be pointed at the same bytes. Does nothing when unset.
#[test]
fn fixtures_can_be_dumped_for_other_tools() {
    let Ok(dir) = std::env::var("STETDOC_DUMP_FIXTURES") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mut index = String::from("kind\tname\texpect\tstage\tpart\treason\texercises\n");
    for f in all() {
        let sub = match f.kind {
            Kind::Edge => "edge",
            Kind::Word => "word",
            Kind::Hostile => "hostile",
        };
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        std::fs::write(dir.join(sub).join(format!("{}.docx", f.name)), f.bytes()).unwrap();
        let reason = if f.reason.is_empty() { "-" } else { f.reason };
        let parts = if f.parts.is_empty() {
            "-".to_string()
        } else {
            f.parts.join(";")
        };
        index.push_str(&format!(
            "{sub}\t{}\t{}\t{}\t{parts}\t{reason}\t{}\n",
            f.name,
            f.expect.code(),
            f.expect.stage_name(),
            f.exercises
        ));
    }
    std::fs::write(dir.join("FIXTURES.tsv"), index).unwrap();
}
