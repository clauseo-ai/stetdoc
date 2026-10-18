//! Tests of the static check of the manifest.

use super::*;
use crate::corpus::manifest::tests::{ERROR_RS, SHA_A, files, violations};
use crate::repo::testutil::repo;
use crate::violation::Violation;

#[test]
fn a_valid_manifest_passes() {
    assert_eq!(violations(&[]), vec![]);
}

#[test]
fn a_committed_document_is_a_violation() {
    let v = violations(&[("crates/x/tests/a.DOCX", "x")]);
    assert_eq!(v.len(), 1);
    assert!(v[0].message.contains("committed"), "{}", v[0]);
}

pub(crate) fn row(tier: &str, expect: &str, stage: &str, reason: &str) -> String {
    let part = if matches!(stage, "read" | "xml") {
        "word/document.xml"
    } else {
        "-"
    };
    format!("{SHA_A}\t10\t{tier}\t{expect}\t{stage}\t{part}\t{reason}\tMIT\tWord\ty\ta.docx\n")
}

fn with_row(text: &str) -> Vec<Violation> {
    violations(&[("corpus/manifest/demo.tsv", text)])
}

fn has(v: &[Violation], needle: &str) -> bool {
    v.iter().any(|v| v.message.contains(needle))
}

#[test]
fn a_refusal_outside_the_hostile_tier_is_a_violation() {
    let v = with_row(&row(
        "word-authored",
        "malformed_input",
        "open",
        "zip.no_eocd",
    ));
    assert!(has(&v, "outside the hostile tier"), "{v:?}");
}

#[test]
fn a_refusal_needs_a_stage_and_a_reason() {
    let v = with_row(&row("hostile", "malformed_input", "-", "zip.no_eocd"));
    assert!(has(&v, "the refusal has stage"), "{v:?}");
    let v = with_row(&row("hostile", "malformed_input", "later", "zip.no_eocd"));
    assert!(has(&v, "the refusal has stage `later`"), "{v:?}");
    let v = with_row(&row("hostile", "limit_exceeded", "xml", "-"));
    assert!(has(&v, "has no reason"), "{v:?}");
    let v = with_row(&row("hostile", "limit_exceeded", "xml", "xml.depth_ge_100"));
    assert_eq!(v, vec![]);
}

#[test]
fn a_reason_must_be_in_the_vocabulary_and_every_listed_reason_counts() {
    let v = with_row(&row(
        "hostile",
        "malformed_input",
        "open",
        "zip.no_eocd;zip.made_up",
    ));
    assert!(
        has(&v, "`zip.made_up` is not in corpus/reasons.tsv"),
        "{v:?}"
    );
    let v = violations(&[(
        "corpus/reasons.tsv",
        "zip.no_eocd\tm\nzip.no_eocd\tagain\nxml.depth_ge_100\t \n",
    )]);
    assert!(has(&v, "listed twice"), "{v:?}");
    assert!(has(&v, "has no meaning"), "{v:?}");
}

#[test]
fn a_hostile_document_that_opens_still_needs_a_reason() {
    let v = with_row(&row("hostile", "ok", "-", "-"));
    assert!(has(&v, "has no reason"), "{v:?}");
    assert_eq!(with_row(&row("hostile", "ok", "-", "zip.no_eocd")), vec![]);
    let v = with_row(&row("lab", "ok", "open", "-"));
    assert!(
        has(&v, "stage `open` on a document that must open"),
        "{v:?}"
    );
}

#[test]
fn the_expected_outcome_must_be_ok_or_a_refusal_error_code() {
    let v = with_row(&row("hostile", "stale_ref", "open", "zip.no_eocd"));
    assert!(has(&v, "unknown expected outcome `stale_ref`"), "{v:?}");
    let v = with_row(&row("hostile", "nonsense", "open", "zip.no_eocd"));
    assert!(has(&v, "unknown expected outcome `nonsense`"), "{v:?}");
}

#[test]
fn the_refusal_codes_are_wire_names_in_the_core() {
    let wires = wire_names_of(ERROR_RS).expect("error.rs parses");
    for code in REFUSAL_CODES {
        assert!(wires.contains(*code), "{code}");
    }
}

#[test]
fn word_fp_is_y_n_or_unknown() {
    let text = format!("{SHA_A}\t10\tlab\tok\t-\t-\t-\tMIT\tWord\tmaybe\ta.docx\n");
    assert!(has(&with_row(&text), "word_fp `maybe`"));
}

#[test]
fn duplicate_hashes_and_bad_hashes_are_reported() {
    let text = format!(
        "{SHA_A}\t10\tlab\tok\t-\t-\t-\tMIT\tx\t?\ta.docx\n{SHA_A}\t10\tlab\tok\t-\t-\t-\tMIT\tx\t?\tb.docx\nxyz\t1\tlab\tok\t-\t-\t-\tMIT\tx\t?\tc.docx\n"
    );
    let v = violations(&[("corpus/manifest/demo.tsv", &text)]);
    assert!(v.iter().any(|v| v.message.contains("already listed")));
    assert!(v.iter().any(|v| v.message.contains("64 lower-case hex")));
}

#[test]
fn a_source_needs_a_pinned_commit_and_a_sources_entry() {
    let v = violations(&[
        (
            "corpus/sources.tsv",
            "demo\thttps://x\tmain\tMIT\tfetch\tyes\n",
        ),
        ("corpus/SOURCES.md", "nothing here\n"),
    ]);
    assert!(v.iter().any(|v| v.message.contains("no pinned commit")));
    assert!(v.iter().any(|v| v.message.contains("has no entry")));
}

#[test]
fn curated_rows_must_exist_in_the_manifest() {
    let v = violations(&[(
        "corpus/curated.tsv",
        &format!("{}\tdemo\tx\tr\n", "c".repeat(64)),
    )]);
    assert!(v.iter().any(|v| v.message.contains("not in the manifest")));
}

fn repo_with(extra: &[(&str, &str)]) -> (tempfile::TempDir, Repo) {
    let owned = files(extra);
    let refs: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    repo(&refs)
}

fn damaged(repo: &Repo) {
    let bad = format!("{SHA_A}\t10\tlab\tok\t-\t-\t-\tMIT\tWord\tmaybe\ta.docx\n");
    repo.write("corpus/manifest/demo.tsv", &bad).expect("write");
}

/// Writes the stamp of a pass for the files as they are now, as if the check had passed.
fn forge_stamp(repo: &Repo) {
    let texts = Texts::read(repo).expect("texts");
    let sources_md = repo.read("corpus/SOURCES.md").expect("sources");
    let error_rs = repo.read(ENUM_FILE).expect("error.rs");
    Stamp::new(repo, &texts, &sources_md, &error_rs).write();
}

#[test]
fn a_pass_leaves_a_stamp_and_a_changed_manifest_is_validated_again() {
    let (dir, repo) = repo_with(&[]);
    let stamp = dir.path().join("target/xtask/corpus-manifest.ok");
    assert_eq!(check_cached(&repo).expect("check"), vec![]);
    assert!(stamp.exists(), "a pass leaves a stamp");
    let key = std::fs::read_to_string(&stamp).expect("stamp");
    assert_eq!(check_cached(&repo).expect("check"), vec![]);
    assert_eq!(std::fs::read_to_string(&stamp).expect("stamp"), key);
    damaged(&repo);
    let v = check_cached(&repo).expect("check");
    assert!(v.iter().any(|v| v.message.contains("word_fp")), "{v:?}");
    assert_eq!(
        std::fs::read_to_string(&stamp).expect("stamp"),
        key,
        "a failure leaves the old stamp, which no longer matches"
    );
}

#[test]
fn the_cached_check_trusts_a_stamp_and_the_full_check_never_does() {
    let (_dir, repo) = repo_with(&[]);
    damaged(&repo);
    forge_stamp(&repo);
    assert_eq!(check_cached(&repo).expect("check"), vec![]);
    assert_eq!(check_manifest_cached(&repo).expect("check"), vec![]);
    let v = check(&repo).expect("check");
    assert!(v.iter().any(|v| v.message.contains("word_fp")), "{v:?}");
}

#[test]
fn the_full_check_leaves_the_stamp_that_lets_the_fetch_skip_validation() {
    let (dir, repo) = repo_with(&[]);
    assert_eq!(check(&repo).expect("check"), vec![]);
    assert!(dir.path().join("target/xtask/corpus-manifest.ok").exists());
}

#[test]
fn the_cached_check_still_finds_committed_documents() {
    let (_dir, repo) = repo_with(&[]);
    forge_stamp(&repo);
    repo.write("crates/x/tests/a.docx", "x").expect("write");
    let v = check_cached(&repo).expect("check");
    assert!(v.iter().any(|v| v.message.contains("committed")), "{v:?}");
}

fn with_part(stage: &str, part: &str) -> Vec<Violation> {
    let text = format!(
        "{SHA_A}\t10\thostile\tmalformed_input\t{stage}\t{part}\tzip.no_eocd\tMIT\tWord\ty\ta.docx\n"
    );
    with_row(&text)
}

#[test]
fn a_read_or_xml_refusal_names_the_part_that_carries_it() {
    for stage in ["read", "xml"] {
        assert_eq!(with_part(stage, "word/theme/theme1.xml"), vec![]);
        assert_eq!(
            with_part(stage, "word/document.xml;docProps/app.xml"),
            vec![]
        );
        assert!(has(&with_part(stage, "-"), "names no part"), "{stage}");
        assert!(has(&with_part(stage, "/word/x.xml"), "is not a part name"));
        assert!(has(&with_part(stage, "a.xml;;b.xml"), "is not a part name"));
    }
}

#[test]
fn only_a_read_or_xml_refusal_has_a_part() {
    for stage in ["open", "document"] {
        assert_eq!(with_part(stage, "-"), vec![]);
        let v = with_part(stage, "word/document.xml");
        assert!(
            has(&v, &format!("on a refusal at the `{stage}` stage")),
            "{v:?}"
        );
    }
    let text = format!("{SHA_A}\t10\tlab\tok\t-\tword/document.xml\t-\tMIT\tWord\ty\ta.docx\n");
    assert!(has(&with_row(&text), "on a document that must open"));
}

#[test]
fn a_compare_pair_names_two_different_documents_that_open_once() {
    let b = crate::corpus::manifest::tests::SHA_B;
    let unknown = "c".repeat(64);
    let pairs = |text: String| violations(&[("corpus/pairs.tsv", &text)]);
    let ok = format!("lab\tcurated\t{SHA_A}\t{unknown}\tdemo\t-\n");
    assert!(has(&pairs(ok), "is not in the manifest"));
    assert!(has(
        &pairs(format!("lab\tfull\t{SHA_A}\t{b}\tdemo\t-\n")),
        "is refused"
    ));
    assert!(has(
        &pairs(format!("lab\tfull\t{SHA_A}\t{SHA_A}\tdemo\t-\n")),
        "with itself"
    ));
    assert!(has(
        &pairs(format!("word\tfull\t{SHA_A}\t{unknown}\tdemo\t-\n")),
        "class `word`"
    ));
    let a2 = "d".repeat(64);
    let manifest = format!(
        "{SHA_A}\t10\tword-authored\tok\t-\t-\t-\tMIT\tWord\ty\ta.docx\n{a2}\t11\tword-authored\tok\t-\t-\t-\tMIT\tWord\ty\tb.docx\n"
    );
    let twice = format!("dox\tfull\t{SHA_A}\t{a2}\tx\t-\ndox\tfull\t{a2}\t{SHA_A}\tx\t-\n");
    let v = violations(&[
        ("corpus/manifest/demo.tsv", &manifest),
        ("corpus/pairs.tsv", &twice),
    ]);
    assert!(has(&v, "already listed at line 1"), "{v:?}");
    let once = format!("dox\tfull\t{SHA_A}\t{a2}\tx\t-\n");
    let v = violations(&[
        ("corpus/manifest/demo.tsv", &manifest),
        ("corpus/pairs.tsv", &once),
    ]);
    assert_eq!(v, vec![]);
}
