//! Offline tests of the shared cache: concurrent fetches, readers during republishing,
//! stale leftovers, a dead network, immutable sets and the warm path.

use super::tests::{World, run_fetch, set_source, world};
use super::*;
use manifest::hex_sha256;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

const A: &[u8] = b"document a bytes";
const B: &[u8] = b"document b is longer";

/// Rewrites the curated list with the documents of `which` ("a", "b" or both) and a note
/// that makes the manifest hash differ between calls.
fn set_curated(w: &World, which: &[&str], note: &str) {
    let mut text = String::new();
    for name in which {
        let (bytes, path) = if *name == "a" {
            (A, "x/a doc.docx")
        } else {
            (B, "y/b.docx")
        };
        text.push_str(&format!(
            "{}\tdemo\tdemo:{path}\tcovers {name} {note}\n",
            hex_sha256(bytes)
        ));
    }
    fs::write(w.repo_dir.path().join("corpus/curated.tsv"), text).expect("write curated");
}

fn hash(w: &World) -> String {
    Manifest::hash_of(&Repo::at(w.repo_dir.path())).expect("hash")
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn two_cold_fetches_into_one_cache_both_succeed_and_download_once() {
    let w = world("fetch", false);
    let repo = Repo::at(w.repo_dir.path());
    for round in 0..5 {
        let cache = tempfile::tempdir().expect("cache");
        let summaries: Vec<Res<Summary>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..2)
                .map(|_| scope.spawn(|| fetch(&repo, cache.path(), &Options::default())))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_else(|_| Err("a fetch panicked".into())))
                .collect()
        });
        let mut downloaded = 0;
        for s in summaries {
            downloaded += s
                .unwrap_or_else(|e| panic!("round {round}: {e}"))
                .downloaded;
        }
        assert_eq!(
            downloaded, 1,
            "round {round}: the second fetch finds the work done"
        );
        assert!(sets::is_published(cache.path(), "curated", &hash(&w)));
        assert!(entries(&cache.path().join("work")).is_empty());
    }
}

#[test]
fn a_reader_of_the_curated_set_never_misses_a_file_while_fetches_republish_it() {
    let w = world("fetch", false);
    run_fetch(&w, false).expect("first fetch");
    let link = sets::set_dir(w.cache.path(), "curated");
    let stop = AtomicBool::new(false);
    let (rounds, fetches) = std::thread::scope(|scope| {
        let reader = scope.spawn(|| -> Res<usize> {
            let mut rounds = 0;
            while !stop.load(Ordering::SeqCst) {
                // Resolve the link once, then read INDEX.tsv and every file from that directory.
                let dir = sets::through_link(|| fs::canonicalize(&link))
                    .map_err(|e| format!("link: {e}"))?;
                let index = sets::through_link(|| fs::read_to_string(dir.join("INDEX.tsv")))
                    .map_err(|e| format!("INDEX.tsv of {}: {e}", dir.display()))?;
                for line in index.lines().skip(1) {
                    let rel = line.split('\t').nth(5).ok_or("a row has no path")?;
                    sets::through_link(|| fs::read(dir.join(rel)))
                        .map_err(|e| format!("{}: {e}", rel))?;
                }
                rounds += 1;
            }
            Ok(rounds)
        });
        let mut fetches = Vec::new();
        for i in 0..15 {
            let which: &[&str] = if i % 2 == 0 { &["a"] } else { &["a", "b"] };
            set_curated(&w, which, &i.to_string());
            fetches.push(run_fetch(&w, false).map(|_| ()));
        }
        stop.store(true, Ordering::SeqCst);
        (
            reader
                .join()
                .unwrap_or_else(|_| Err("the reader panicked".into())),
            fetches,
        )
    });
    for f in fetches {
        f.expect("every fetch succeeds");
    }
    let rounds = rounds.expect("the reader never saw a missing file or index");
    assert!(rounds > 0, "the reader ran");
}

/// Reads the set the way the testkit does or the way a plain walker does, over and over, and
/// counts what goes wrong. Every open goes through [`sets::through_link`], as the module asks
/// of a reader; nothing else is forgiven.
fn read_set_until(link: &Path, pinned: bool, stop: &AtomicBool) -> (usize, Vec<String>) {
    let mut rounds = 0;
    let mut errors = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        let dir = if pinned {
            sets::through_link(|| fs::canonicalize(link)).unwrap_or_else(|_| link.to_path_buf())
        } else {
            link.to_path_buf()
        };
        match sets::through_link(|| fs::read_to_string(dir.join("INDEX.tsv"))) {
            Err(e) => errors.push(format!("INDEX.tsv: {e}")),
            Ok(index) => {
                for line in index.lines().skip(1) {
                    let rel = line.split('\t').nth(5).unwrap_or("");
                    if let Err(e) = sets::through_link(|| fs::read(dir.join(rel))) {
                        errors.push(format!("{rel}: {e}"));
                    }
                }
            }
        }
        rounds += 1;
    }
    (rounds, errors)
}

#[test]
fn readers_never_miss_a_file_while_bad_blobs_are_repaired() {
    let w = world("fetch", false);
    let root = w.cache.path();
    run_fetch(&w, false).expect("first fetch");
    let link = sets::set_dir(root, "curated");
    let blob = cache::blob_path(root, &hex_sha256(A));
    let stop = AtomicBool::new(false);
    let (pinned, unpinned, repairs) = std::thread::scope(|scope| {
        let pinned = scope.spawn(|| read_set_until(&link, true, &stop));
        let unpinned = scope.spawn(|| read_set_until(&link, false, &stop));
        let mut repairs = Vec::new();
        for i in 0..40 {
            fs::remove_file(&blob).expect("remove the blob");
            let damaged = format!("document a BYTES {i:>2}");
            fs::write(&blob, &damaged.as_bytes()[..A.len()]).expect("corrupt it, same size");
            repairs.push(run_fetch(&w, false).map(|s| s.downloaded));
        }
        stop.store(true, Ordering::SeqCst);
        (
            pinned.join().unwrap_or_default(),
            unpinned.join().unwrap_or_default(),
            repairs,
        )
    });
    for r in repairs {
        assert_eq!(r.expect("every repair succeeds"), 1);
    }
    assert!(pinned.0 > 0 && unpinned.0 > 0, "the readers ran");
    let first = |errors: &[String]| errors.iter().take(3).cloned().collect::<Vec<_>>();
    assert!(
        pinned.1.is_empty(),
        "a reader that resolved the set met {} errors, such as {:?}",
        pinned.1.len(),
        first(&pinned.1)
    );
    assert!(
        unpinned.1.is_empty(),
        "a reader of the link met {} errors, such as {:?}",
        unpinned.1.len(),
        first(&unpinned.1)
    );
}

/// Makes a file writable for its owner, as a blob stored by the first layout was.
fn make_writable(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).expect("make it writable");
    }
    #[cfg(not(unix))]
    {
        let mut permissions = fs::metadata(path).expect("file").permissions();
        #[expect(
            clippy::permissions_set_readonly_false,
            reason = "a test file on Windows"
        )]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions).expect("make it writable");
    }
}

/// Damages the blob of document a in place, as bit rot would: the sets that link to it see the
/// damage too.
fn corrupt_blob_a(w: &World) {
    let blob = cache::blob_path(w.cache.path(), &hex_sha256(A));
    make_writable(&blob);
    fs::write(&blob, b"document a BYTES").expect("corrupt it, same size");
}

fn linked_a(dir: &Path) -> PathBuf {
    dir.join("word-authored")
        .join(format!("{}-a_doc.docx", &hex_sha256(A)[..12]))
}

#[test]
fn a_repair_builds_a_new_generation_and_keeps_the_old_one_for_readers_for_a_day() {
    let w = world("fetch", false);
    let root = w.cache.path();
    run_fetch(&w, false).expect("first fetch");
    let link = sets::set_dir(root, "curated");
    let first = fs::canonicalize(&link).expect("generation");
    assert!(
        first
            .to_string_lossy()
            .contains(&format!("{}.", sets::set_name("curated", &hash(&w))))
    );

    corrupt_blob_a(&w);
    // A reader that resolved the set before the repair holds this directory.
    let reading = fs::canonicalize(&link).expect("generation");
    run_fetch(&w, false).expect("repair");
    let second = fs::canonicalize(&link).expect("generation");
    assert_ne!(first, second, "the repair is a new generation");
    assert_eq!(fs::read(linked_a(&second)).expect("repaired"), A);
    assert_eq!(
        fs::read(linked_a(&reading)).expect("the reader's directory is untouched"),
        b"document a BYTES",
        "the old generation is exactly as it was, so a reader sees no change under it"
    );
    assert!(reading.join("INDEX.tsv").exists() && reading.join("COMPLETE").exists());
    let stable = sets::immutable_dir(root, "curated", &hash(&w));
    assert_eq!(
        fs::read_link(&stable).expect("stable link"),
        Path::new(second.file_name().expect("name"))
    );

    let two_days = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
    fs::File::open(&reading)
        .and_then(|f| f.set_modified(two_days))
        .expect("age the old generation");
    corrupt_blob_a(&w);
    run_fetch(&w, false).expect("second repair prunes the old generation");
    assert!(
        !reading.exists(),
        "the old generation is pruned once it is a day old"
    );
    assert!(
        second.exists(),
        "the generation before this repair is young and stays"
    );
}

#[test]
fn a_set_directory_of_the_previous_layout_is_kept_until_a_repair_turns_it_into_a_generation() {
    let w = world("fetch", false);
    let root = w.cache.path();
    run_fetch(&w, false).expect("first fetch");
    // Rebuild what the previous layout looked like: the stable name is a real directory.
    let stable = sets::immutable_dir(root, "curated", &hash(&w));
    let generation = fs::canonicalize(&stable).expect("generation");
    fs::remove_file(&stable).expect("remove the link");
    fs::rename(&generation, &stable).expect("the set itself under the stable name");
    assert!(
        !stable
            .symlink_metadata()
            .expect("meta")
            .file_type()
            .is_symlink()
    );

    run_fetch(&w, false).expect("a warm fetch leaves it alone");
    assert!(
        !stable
            .symlink_metadata()
            .expect("meta")
            .file_type()
            .is_symlink()
    );

    corrupt_blob_a(&w);
    run_fetch(&w, false).expect("the repair converts it");
    assert!(
        stable
            .symlink_metadata()
            .expect("meta")
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(linked_a(&stable)).expect("repaired"), A);
    let kept: Vec<String> = entries(&root.join("sets"))
        .into_iter()
        .filter(|n| n.starts_with(&format!("{}.", sets::set_name("curated", &hash(&w)))))
        .collect();
    assert_eq!(
        kept.len(),
        2,
        "the old directory and the new generation: {kept:?}"
    );
    for name in kept {
        assert!(root.join("sets").join(name).join("INDEX.tsv").exists());
    }
}

#[test]
fn blobs_stored_by_an_earlier_version_are_made_read_only() {
    let w = world("fetch", false);
    let root = w.cache.path();
    run_fetch(&w, false).expect("first fetch");
    let blob = cache::blob_path(root, &hex_sha256(A));
    make_writable(&blob);
    assert!(!fs::metadata(&blob).expect("blob").permissions().readonly());
    let s = run_fetch(&w, false).expect("the next fetch protects it");
    assert_eq!(s.downloaded, 0, "nothing is downloaded for a writable blob");
    assert!(fs::metadata(&blob).expect("blob").permissions().readonly());
}

#[test]
fn leftovers_of_killed_runs_do_not_stop_a_fetch_and_are_removed() {
    let w = world("fetch", false);
    let root = w.cache.path();
    fs::create_dir_all(root.join("work/123-0/demo")).expect("stale work");
    fs::write(root.join("work/123-0/demo/shallow.lock"), "").expect("shallow.lock");
    fs::create_dir_all(root.join("git/demo")).expect("old git dir");
    fs::write(root.join("git/demo/shallow.lock"), "").expect("old shallow.lock");
    fs::create_dir_all(root.join("sets/.tmp-curated-99")).expect("half-built set");
    fs::create_dir_all(root.join("blobs/aa")).expect("shard");
    fs::write(root.join("blobs/aa/.tmp-x-99"), "half").expect("half blob");
    run_fetch(&w, false).expect("fetch succeeds despite the leftovers");
    assert!(!root.join("work/123-0").exists());
    assert!(!root.join("git").exists());
    assert!(!root.join("sets/.tmp-curated-99").exists());
    assert!(!root.join("blobs/aa/.tmp-x-99").exists());
}

#[test]
fn an_unreachable_upstream_fails_fast_in_one_line() {
    let w = world("fetch", false);
    set_source(&w, Some("file:///no/such/upstream/for/the/test"), None);
    let started = std::time::Instant::now();
    let err = run_fetch(&w, false).expect_err("must fail");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "returned quickly"
    );
    assert!(!err.contains('\n'), "one line: {err}");
    assert!(
        err.starts_with("cannot reach file:///no/such/upstream/for/the/test ("),
        "{err}"
    );
    assert!(err.contains(&w.cache.path().display().to_string()), "{err}");
    assert!(err.contains("is missing 1 of 1 documents"), "{err}");
    assert!(err.contains("`cargo xtask corpus fetch`"), "{err}");
    assert!(err.contains("network"), "{err}");
    assert!(!w.cache.path().join("sets").join("curated").exists());
    assert!(entries(&w.cache.path().join("work")).is_empty());
}

#[test]
fn a_bad_manifest_is_refused_before_git_runs_or_a_file_is_made() {
    let w = world("fetch", false);
    fs::remove_dir_all(&w.up_path).expect("no upstream, so any git command would fail");
    let path = w.repo_dir.path().join("corpus/manifest/demo.tsv");
    let text = fs::read_to_string(&path).expect("read");
    fs::write(&path, text.replace("word-authored", "../escape")).expect("write");
    let err = run_fetch(&w, false).expect_err("must refuse");
    assert!(err.contains("unknown tier `../escape`"), "{err}");
    assert!(!err.contains("cannot reach"), "{err}");
    assert!(
        entries(w.cache.path()).is_empty(),
        "nothing was created: {:?}",
        entries(w.cache.path())
    );
}

#[test]
fn a_complete_cache_is_served_without_the_lock_or_the_upstream() {
    let w = world("fetch", false);
    run_fetch(&w, false).expect("first fetch");
    fs::remove_dir_all(&w.up_path).expect("upstream disappears");
    let lock = w.cache.path().join(".lock");
    fs::remove_file(&lock).expect("remove the lock file");
    let s = run_fetch(&w, false).expect("warm fetch");
    assert_eq!((s.downloaded, s.cached), (0, 1));
    assert!(!lock.exists(), "the warm path took no lock");
    assert!(!w.cache.path().join("work").exists());
}

#[test]
fn the_warm_path_does_not_wait_for_a_held_lock() {
    let w = world("fetch", false);
    run_fetch(&w, false).expect("first fetch");
    let held = lock::acquire(w.cache.path()).expect("take the lock");
    run_fetch(&w, false).expect("the warm path needs no lock");
    drop(held);
}

#[test]
fn sets_are_immutable_directories_behind_a_symlink_and_old_ones_are_pruned_after_a_day() {
    let w = world("fetch", false);
    let root = w.cache.path();
    run_fetch(&w, false).expect("first fetch");
    let first = sets::set_name("curated", &hash(&w));
    let first_dir = root.join("sets").join(&first);
    assert!(sets::set_complete(root, "curated", &hash(&w)));
    assert!(first_dir.join("COMPLETE").exists() && first_dir.join("INDEX.tsv").exists());
    let link = sets::set_dir(root, "curated");
    assert!(
        link.symlink_metadata()
            .expect("meta")
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&link).expect("link"), Path::new(&first));
    assert_eq!(first.len(), "curated-".len() + 12);
    assert!(first.starts_with("curated-"));

    set_curated(&w, &["a", "b"], "second");
    run_fetch(&w, false).expect("second fetch");
    let second = sets::set_name("curated", &hash(&w));
    assert_ne!(first, second);
    assert_eq!(fs::read_link(&link).expect("link"), Path::new(&second));
    assert!(
        first_dir.exists(),
        "the old set is younger than a day, a reader may use it"
    );
    assert_eq!(fs::read_dir(link.join("lab")).expect("lab").count(), 1);

    let two_days = SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
    fs::File::open(&first_dir)
        .and_then(|f| f.set_modified(two_days))
        .expect("age the old set");
    set_curated(&w, &["b"], "third");
    run_fetch(&w, false).expect("third fetch");
    let third = sets::set_name("curated", &hash(&w));
    assert!(!first_dir.exists(), "the old set is pruned");
    assert!(
        root.join("sets").join(&second).exists(),
        "the young one stays"
    );
    assert_eq!(fs::read_link(&link).expect("link"), Path::new(&third));
    assert!(
        entries(&root.join("sets"))
            .iter()
            .all(|n| !n.starts_with(".tmp-")),
        "{:?}",
        entries(&root.join("sets"))
    );
}

#[test]
fn a_directory_of_the_previous_layout_becomes_a_symlink() {
    let w = world("fetch", false);
    let old = sets::set_dir(w.cache.path(), "curated");
    fs::create_dir_all(&old).expect("old set");
    fs::write(old.join("COMPLETE"), "old\n").expect("old marker");
    run_fetch(&w, false).expect("fetch");
    assert!(
        old.symlink_metadata()
            .expect("meta")
            .file_type()
            .is_symlink()
    );
    assert!(old.join("INDEX.tsv").exists());
}

#[test]
fn a_repaired_blob_is_linked_into_a_rebuilt_set() {
    let w = world("fetch", false);
    let root = w.cache.path();
    run_fetch(&w, false).expect("first fetch");
    let blob = cache::blob_path(root, &hex_sha256(A));
    fs::remove_file(&blob).expect("remove");
    fs::write(&blob, b"document a BYTES").expect("corrupt with the same size");
    run_fetch(&w, false).expect("repair");
    let linked = sets::set_dir(root, "curated")
        .join("word-authored")
        .join(format!("{}-a_doc.docx", &hex_sha256(A)[..12]));
    assert_eq!(fs::read(linked).expect("linked"), A);
}
