//! Offline tests of `corpus fetch` against a local git repository standing in for GitHub.

use super::*;
use crate::repo::testutil::repo;
use manifest::hex_sha256;
use std::fs;
use std::process::Command;

/// The real error vocabulary, which the manifest check reads the refusal codes from.
const ERROR_RS: &str = include_str!("../../../crates/stetdoc-core/src/api/error.rs");

pub(super) fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.test"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// An upstream repository with these files, and its commit.
fn upstream(files: &[(&str, &[u8])]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("dir");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    git(dir.path(), &["config", "uploadpack.allowFilter", "true"]);
    git(
        dir.path(),
        &["config", "uploadpack.allowAnySHA1InWant", "true"],
    );
    for (path, bytes) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().expect("parent")).expect("mkdir");
        fs::write(full, bytes).expect("write");
    }
    git(dir.path(), &["add", "."]);
    git(dir.path(), &["commit", "-q", "-m", "files"]);
    let commit = git(dir.path(), &["rev-parse", "HEAD"]);
    (dir, commit)
}

pub(super) struct World {
    _up: tempfile::TempDir,
    pub(super) up_path: PathBuf,
    pub(super) repo_dir: tempfile::TempDir,
    pub(super) cache: tempfile::TempDir,
}

pub(super) fn row(bytes: &[u8], tier: &str, path: &str) -> String {
    format!(
        "{}\t{}\t{tier}\tok\t-\t-\t-\tMIT\tWord\ty\t{path}\n",
        hex_sha256(bytes),
        bytes.len()
    )
}

/// Two documents upstream, one in the curated list. `tamper` damages the manifest hash.
pub(super) fn world(how: &str, tamper: bool) -> World {
    let a: &[u8] = b"document a bytes";
    let b: &[u8] = b"document b is longer";
    let (up, commit) = upstream(&[("x/a doc.docx", a), ("y/b.docx", b)]);
    let up_path = up.path().to_path_buf();
    let mut manifest = row(a, "word-authored", "x/a doc.docx");
    manifest.push_str(&row(b, "lab", "y/b.docx"));
    if tamper {
        manifest = manifest.replace(&hex_sha256(a), &hex_sha256(b"something else"));
    }
    let sources = format!(
        "demo\tfile://{}\t{commit}\tMIT\t{how}\tyes\n",
        up_path.display()
    );
    let listed = if tamper {
        hex_sha256(b"something else")
    } else {
        hex_sha256(a)
    };
    let curated = format!("{listed}\tdemo\tdemo:x/a doc.docx\tcovers a\n");
    let (repo_dir, _) = repo(&[
        ("corpus/sources.tsv", &sources),
        ("corpus/manifest/demo.tsv", &manifest),
        ("corpus/curated.tsv", &curated),
        ("corpus/SOURCES.md", "| demo | x |\n"),
        (
            "corpus/reasons.tsv",
            "zip.no_eocd\tthere is no end record\n",
        ),
        ("crates/stetdoc-core/src/api/error.rs", ERROR_RS),
    ]);
    World {
        _up: up,
        up_path,
        repo_dir,
        cache: tempfile::tempdir().expect("cache"),
    }
}

pub(super) fn run_fetch(w: &World, full: bool) -> Res<Summary> {
    let repo = Repo::at(w.repo_dir.path());
    fetch(
        &repo,
        w.cache.path(),
        &Options {
            full,
            ..Options::default()
        },
    )
}

#[test]
fn the_curated_corpus_downloads_verifies_and_builds_a_set() {
    let w = world("fetch", false);
    let s = run_fetch(&w, false).expect("fetch");
    assert_eq!((s.docs, s.downloaded, s.cached), (1, 1, 0));
    let set = sets::set_dir(w.cache.path(), "curated");
    let linked = set.join("word-authored").join(format!(
        "{}-a_doc.docx",
        &hex_sha256(b"document a bytes")[..12]
    ));
    assert_eq!(fs::read(linked).expect("linked file"), b"document a bytes");
    let index = fs::read_to_string(set.join("INDEX.tsv")).expect("index");
    assert!(
        index.contains("demo:x/a doc.docx\tword-authored\tok\t"),
        "{index}"
    );
    assert!(
        !w.cache.path().join("git").exists(),
        "the shared git directory of the old layout is gone"
    );
    let left: Vec<_> = fs::read_dir(w.cache.path().join("work"))
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "the work directory is removed: {left:?}");
}

#[test]
fn a_complete_cache_needs_no_network() {
    let w = world("fetch", false);
    run_fetch(&w, false).expect("first fetch");
    fs::remove_dir_all(&w.up_path).expect("upstream disappears");
    let s = run_fetch(&w, false).expect("offline fetch");
    assert_eq!((s.downloaded, s.cached), (0, 1));
}

#[test]
fn a_corrupted_cached_file_is_fetched_again() {
    let w = world("fetch", false);
    run_fetch(&w, false).expect("first fetch");
    let blob = cache::blob_path(w.cache.path(), &hex_sha256(b"document a bytes"));
    fs::remove_file(&blob).expect("blobs are read-only, so the test replaces the file");
    fs::write(&blob, b"document a BYTES").expect("corrupt");
    let s = run_fetch(&w, false).expect("repairs");
    assert_eq!(s.downloaded, 1);
    assert_eq!(fs::read(blob).expect("blob"), b"document a bytes");
}

#[test]
fn the_full_corpus_includes_documents_outside_the_curated_list() {
    let w = world("fetch", false);
    let s = run_fetch(&w, true).expect("full fetch");
    assert_eq!((s.docs, s.downloaded), (2, 2));
    assert!(sets::set_complete(
        w.cache.path(),
        "full",
        &Manifest::read(&Repo::at(w.repo_dir.path()))
            .expect("m")
            .hash
    ));
    assert!(sets::set_complete(
        w.cache.path(),
        "curated",
        &Manifest::read(&Repo::at(w.repo_dir.path()))
            .expect("m")
            .hash
    ));
}

#[test]
fn a_wrong_hash_in_the_manifest_fails_and_names_both_hashes() {
    let w = world("fetch", true);
    let err = run_fetch(&w, false).expect_err("must fail");
    assert!(err.contains("SHA-256"), "{err}");
    assert!(err.contains(&hex_sha256(b"something else")), "{err}");
    assert!(err.contains(&hex_sha256(b"document a bytes")), "{err}");
    assert!(!cache::blob_path(w.cache.path(), &hex_sha256(b"something else")).exists());
}

/// Rewrites the url and the commit of the only source of a `world`.
pub(super) fn set_source(w: &World, url: Option<&str>, commit: Option<&str>) {
    let path = w.repo_dir.path().join("corpus/sources.tsv");
    let sources = fs::read_to_string(&path).expect("read");
    let columns: Vec<&str> = sources.trim_end().split('\t').collect();
    let bad = [
        columns[0],
        url.unwrap_or(columns[1]),
        commit.unwrap_or(columns[2]),
        columns[3],
        columns[4],
        columns[5],
    ]
    .join("\t");
    fs::write(&path, format!("{bad}\n")).expect("write");
}

#[test]
fn a_pin_the_source_does_not_have_fails_in_one_line_naming_the_commit_and_the_url() {
    let w = world("fetch", false);
    let zeros = "0".repeat(40);
    set_source(&w, None, Some(&zeros));
    let err = run_fetch(&w, false).expect_err("must fail");
    assert!(!err.contains('\n'), "one line: {err}");
    assert!(
        err.starts_with(&format!(
            "the pinned commit {zeros} is not available from file://"
        )),
        "{err}"
    );
    assert!(err.contains(&w.up_path.display().to_string()), "{err}");
    assert!(err.contains("(source demo)"), "{err}");
    assert!(err.contains("corpus/sources.tsv"), "{err}");
}

#[test]
fn a_failing_sink_is_reported_as_a_problem_and_stops_git_cleanly() {
    let w = world("fetch", false);
    let repo = Repo::at(w.repo_dir.path());
    let m = Manifest::read(&repo).expect("manifest");
    let source = m.source("demo").expect("source");
    let docs: Vec<&Doc> = m.docs.iter().collect();
    let abort = std::sync::atomic::AtomicBool::new(false);
    let work = tempfile::tempdir().expect("work");
    let err = gitfetch::fetch(
        &work.path().join("demo"),
        &source.url,
        &source.commit,
        &docs,
        &abort,
        |_, _| Err("disk full".into()),
    )
    .expect_err("the sink fails");
    let text = err.to_string();
    assert!(
        text.contains("cannot store the verified blob: disk full"),
        "{text}"
    );
    assert!(
        !abort.load(std::sync::atomic::Ordering::SeqCst),
        "a sink error does not abort the other sources"
    );
}

#[test]
fn the_documents_of_compare_pairs_join_the_curated_set_or_the_pairs_set() {
    let w = world("fetch", false);
    let (a, b) = (
        hex_sha256(b"document a bytes"),
        hex_sha256(b"document b is longer"),
    );
    let pairs = w.repo_dir.path().join("corpus/pairs.tsv");
    fs::write(&pairs, format!("lab\tfull\t{a}\t{b}\tdemo\t-\n")).expect("pairs");
    let s = run_fetch(&w, false).expect("fetch");
    assert_eq!(
        (s.docs, s.downloaded),
        (1, 1),
        "a full pair is not fetched with the curated corpus"
    );
    let repo = Repo::at(w.repo_dir.path());
    let opts = Options {
        pairs: true,
        ..Options::default()
    };
    let s = fetch(&repo, w.cache.path(), &opts).expect("fetch with --pairs");
    assert_eq!((s.docs, s.downloaded), (2, 1));
    let hash = Manifest::read(&repo).expect("m").hash;
    assert!(sets::set_complete(w.cache.path(), "pairs", &hash));
    let index = fs::read_to_string(sets::set_dir(w.cache.path(), "pairs").join("INDEX.tsv"))
        .expect("index");
    assert!(index.contains("demo:y/b.docx"), "{index}");
    fs::write(&pairs, format!("lab\tcurated\t{a}\t{b}\tdemo\t-\n")).expect("pairs");
    let s = run_fetch(&w, false).expect("fetch");
    assert_eq!(s.docs, 1, "the curated set itself does not change");
    let hash = Manifest::read(&repo).expect("m").hash;
    let index = fs::read_to_string(
        sets::immutable_dir(w.cache.path(), "curatedpairs", &hash).join("INDEX.tsv"),
    )
    .expect("the curated pairs have a set of their own");
    assert!(
        index.contains("demo:x/a doc.docx") && index.contains("demo:y/b.docx"),
        "{index}"
    );
}

#[test]
fn an_outlier_reason_does_not_rename_the_sets() {
    let w = world("fetch", false);
    let repo = Repo::at(w.repo_dir.path());
    let (a, b) = (
        hex_sha256(b"document a bytes"),
        hex_sha256(b"document b is longer"),
    );
    let pairs = w.repo_dir.path().join("corpus/pairs.tsv");
    fs::write(&pairs, format!("lab\tcurated\t{a}\t{b}\tdemo\t-\n")).expect("pairs");
    let before = Manifest::read(&repo).expect("m").hash;
    fs::write(&pairs, format!("lab\tcurated\t{a}\t{b}\tdemo\tshort\n")).expect("pairs");
    assert_eq!(Manifest::read(&repo).expect("m").hash, before);
    fs::write(&pairs, format!("lab\tfull\t{a}\t{b}\tdemo\tshort\n")).expect("pairs");
    assert_ne!(Manifest::read(&repo).expect("m").hash, before);
}

#[test]
fn host_only_documents_are_counted_and_left_out() {
    let w = world("host", false);
    let s = run_fetch(&w, false).expect("fetch");
    assert_eq!((s.docs, s.downloaded, s.host_only), (0, 0, 1));
}

#[test]
fn flags_parse_and_unknown_flags_are_rejected() {
    let o = parse_flags(&["--full", "--verify", "--dir", "/x", "--jobs", "2"]).expect("parse");
    assert!(o.full && o.verify && !o.pairs);
    assert!(parse_flags(&["--pairs"]).expect("parse").pairs);
    assert_eq!((o.dir.as_deref(), o.jobs), (Some("/x"), 2));
    assert!(parse_flags(&["--bogus"]).is_err());
    assert!(parse_flags(&["--dir"]).is_err());
}
