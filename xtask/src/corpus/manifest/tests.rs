//! Tests of the manifest parser, hash and file names. The helpers build small repositories and
//! are shared with the tests of the check in `validate`.

use super::*;
use crate::repo::testutil::repo;

pub(crate) const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(crate) const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
pub(crate) const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// The real error vocabulary, so the refusal codes are checked against `ErrorCode`.
pub(crate) const ERROR_RS: &str = include_str!("../../../../crates/stetdoc-core/src/api/error.rs");

pub(crate) fn files(extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut f = vec![
        (
            "corpus/sources.tsv".to_string(),
            format!("# source\ndemo\thttps://example.test/demo\t{COMMIT}\tMIT\tfetch\tyes\n"),
        ),
        (
            "corpus/manifest/demo.tsv".to_string(),
            format!(
                "{SHA_A}\t10\tword-authored\tok\t-\t-\t-\tMIT\tMicrosoft Word\ty\ta/b c.docx\n{SHA_B}\t20\thostile\tmalformed_input\topen\t-\tzip.no_eocd\tMIT\tunknown\t?\ta/d.docx\n"
            ),
        ),
        (
            "corpus/curated.tsv".to_string(),
            format!("{SHA_A}\tdemo\tdemo:b c.docx\tcovers x\n"),
        ),
        (
            "corpus/SOURCES.md".to_string(),
            "| source | url |\n| demo | https://example.test/demo |\n".to_string(),
        ),
        (
            "corpus/reasons.tsv".to_string(),
            "# reason\tmeaning\nzip.no_eocd\tthere is no end of central directory record\nxml.depth_ge_100\tthe XML nests 100 levels or more\n".to_string(),
        ),
        (
            "crates/stetdoc-core/src/api/error.rs".to_string(),
            ERROR_RS.to_string(),
        ),
    ];
    for (p, c) in extra {
        f.retain(|(q, _)| q != p);
        f.push((p.to_string(), c.to_string()));
    }
    f
}

pub(crate) fn violations(extra: &[(&str, &str)]) -> Vec<crate::violation::Violation> {
    let owned = files(extra);
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (_dir, repo) = repo(&refs);
    crate::corpus::validate::check(&repo).expect("check runs")
}

#[test]
fn malformed_rows_fail_to_parse_with_the_line_number() {
    let err = parse_docs("demo", "corpus/manifest/demo.tsv", "a\tb\n").expect_err("short row");
    assert!(err.contains("demo.tsv:1"), "{err}");
    let err = parse_reasons("only one column\n").expect_err("short row");
    assert!(err.contains("reasons.tsv:1"), "{err}");
    assert!(parse_sources("x\ty\tz\tw\tbogus\tyes\n").is_err());
}

#[test]
fn file_names_are_safe_and_unique_by_hash() {
    let doc = Doc {
        sha256: SHA_A.into(),
        size: 1,
        tier: "lab".into(),
        expect: "ok".into(),
        stage: "-".into(),
        part: "-".into(),
        reason: "-".into(),
        spdx: "MIT".into(),
        producer: String::new(),
        word_fp: "?".into(),
        path: "dir/..weird name (1).docx".into(),
        source: "demo".into(),
        origin: (String::new(), 0),
    };
    assert_eq!(doc.file_name(), "aaaaaaaaaaaa-weird_name__1_.docx");
}

#[test]
fn the_manifest_hash_changes_with_any_file() {
    let owned = files(&[]);
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (_d1, r1) = repo(&refs);
    let before = Manifest::read(&r1).expect("read").hash;
    let changed = files(&[("corpus/curated.tsv", "")]);
    let refs2: Vec<(&str, &str)> = changed
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (_d2, r2) = repo(&refs2);
    assert_ne!(before, Manifest::read(&r2).expect("read").hash);
    assert_eq!(Manifest::hash_of(&r1).expect("hash"), before);
    let reworded = files(&[("corpus/reasons.tsv", "zip.no_eocd\tworded differently\n")]);
    let refs3: Vec<(&str, &str)> = reworded
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    let (_d3, r3) = repo(&refs3);
    assert_eq!(
        Manifest::hash_of(&r3).expect("hash"),
        before,
        "the vocabulary does not name the sets"
    );
    assert_eq!(
        hex_sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}
