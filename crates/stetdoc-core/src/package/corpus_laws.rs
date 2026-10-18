//! Laws L1 and L2 for the container, over the fetched corpus.
//!
//! The repository commits no third-party documents. `cargo xtask corpus fetch`
//! downloads the curated corpus by pinned hash into a cache, and these tests read it from the
//! directories named in `STETDOC_CORPUS_DIR`, separated by `:`. For each directory, `sets/curated`
//! (or `sets/<name>` when `STETDOC_CORPUS_SET` is set) is used if it exists, and otherwise the
//! directory itself is walked, so a plain folder of documents works too. With the variable unset
//! the tests report that they skipped and pass; `cargo xtask check` sets the variable for
//! the tests in scope, so the skip is not a way out of CI.
//!
//! A set that is named has to be a whole one, or the tests fail: each directory needs a `COMPLETE`
//! marker, which the fetch writes last, and at least one document. A document that cannot be read
//! is a failure, and so is a mismatch between the files and `INDEX.tsv`.
//!
//! `INDEX.tsv` has a header row. The columns used are found by name: `path` (relative to the set),
//! `expect` (`ok`, or the error code the document must be refused with) and, when there are
//! them, `stage` (`open`, `read`, `xml` or `document`) and `part` (the parts that carry a refusal
//! at `read` or `xml`, joined by `;`, or `-`). Other columns are ignored. A document the index
//! expects to be refused has to be refused as it says, and the test fails if it is not, or if it is
//! refused earlier than the index says:
//! - `open`: `Package::open` returns exactly that code.
//! - `read`: `open` succeeds and a part is refused with that code when it is read. With `part`,
//!   it is one of those.
//! - `xml`: `open` succeeds, and an XML part is refused with that code when it is parsed. With
//!   `part`, it is one of those, and it reads.
//! - `document`: the refusal comes from layers above this one, so `open` has to succeed and the
//!   container laws apply as to any other document.
//! - no stage: any of the first three.
//!
//! Without an index, a document under a directory named `hostile` has to fail cleanly: `open`
//! returns an error, or the document opens and every part either reads or returns an error, and
//! nothing panics. Every other document, and every one the index expects to be `ok`, must satisfy:
//! 1. L1, shortcut: `write` returns the input bytes exactly.
//! 2. L1, rebuild: `write_rebuilt` returns the input bytes exactly, so the code that edits depend
//!    on is exact when nothing has changed.
//! 3. Replacing every part with its own bytes gives a package that opens, whose parts read back the
//!    same, and whose untouched entries are raw-identical.
//! 4. L2 smoke: changing the text of one `w:t` in the main part changes that part and nothing else:
//!    every other entry is raw-identical and every other central record differs at most in its
//!    local header offset.
//! 5. Another reader agrees. `reference_reader`, which follows Python's `zipfile` and shares no code
//!    with this module, reads the input, and every archive laws 3 and 4 write, to the same entries:
//!    the same names in the same order, with the same sizes and CRC-32 values. A package that
//!    opens here and reads differently there is a package that Word, `unzip` or `bsdtar` could
//!    show something else, and an edit of it could leave a file they refuse.
//!
//! The same laws run on built archives in `tests.rs`, with every ZIP structure we know of.

use super::reference_reader as reference;
use super::*;
use crate::xml::SpanTree;
use refusal::check_refusal;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const EXTENSIONS: [&str; 4] = ["docx", "docm", "dotx", "dotm"];

/// What `INDEX.tsv` says about one document.
#[derive(Debug)]
struct Expect {
    /// `ok`, or the wire name of an error code.
    code: String,
    stage: Option<String>,
    /// The parts that carry a refusal at `read` or `xml`. Empty when the index names none.
    parts: Vec<String>,
}

/// One directory of documents, with its index if it has one.
#[derive(Debug)]
struct CorpusSet {
    dir: PathBuf,
    docs: Vec<PathBuf>,
    index: Option<HashMap<PathBuf, Expect>>,
}

/// The index, by path. The columns are found by their names in the header row.
fn parse_index(text: &str) -> Result<HashMap<PathBuf, Expect>, String> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (Some(path), Some(expect)) = (col("path"), col("expect")) else {
        return Err("INDEX.tsv has no `path` and `expect` columns in its header".to_owned());
    };
    let (stage, part) = (col("stage"), col("part"));
    let mut out = HashMap::new();
    for (n, line) in lines.enumerate().filter(|(_, l)| !l.is_empty()) {
        let cols: Vec<&str> = line.split('\t').collect();
        let (Some(p), Some(e)) = (cols.get(path), cols.get(expect)) else {
            return Err(format!("INDEX.tsv row {} has too few columns", n + 2));
        };
        let row = Expect {
            code: (*e).to_owned(),
            stage: stage
                .and_then(|s| cols.get(s))
                .filter(|s| !s.is_empty())
                .map(|s| (*s).to_owned()),
            parts: part
                .and_then(|c| cols.get(c))
                .filter(|p| !p.is_empty() && **p != "-")
                .map(|p| p.split(';').map(str::to_owned).collect())
                .unwrap_or_default(),
        };
        if out.insert(PathBuf::from(p), row).is_some() {
            return Err(format!("INDEX.tsv lists {p} twice"));
        }
    }
    Ok(out)
}

#[cfg(test)]
fn documents(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|k| k.is_dir()) {
                stack.push(path);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// The sets named by `STETDOC_CORPUS_DIR`. `Ok(None)` if it is not set. An error says why a set
/// that is named cannot be used.
#[cfg(test)]
fn load_sets() -> Result<Option<Vec<CorpusSet>>, String> {
    let Ok(dirs) = std::env::var("STETDOC_CORPUS_DIR") else {
        return Ok(None);
    };
    let set = std::env::var("STETDOC_CORPUS_SET").unwrap_or_else(|_| "curated".to_owned());
    load_from(&dirs, &set).map(Some)
}

/// The sets in a `:`-separated list of directories, each resolved to `sets/<set>` if it has one.
#[cfg(test)]
fn load_from(dirs: &str, set: &str) -> Result<Vec<CorpusSet>, String> {
    let named: Vec<&str> = dirs.split(':').filter(|s| !s.is_empty()).collect();
    if named.is_empty() {
        return Err("STETDOC_CORPUS_DIR is set but names no directory".to_owned());
    }
    let mut sets = Vec::new();
    for d in named {
        let curated = Path::new(d).join("sets").join(set);
        let dir = if curated.is_dir() {
            curated
        } else {
            PathBuf::from(d)
        };
        if !dir.join("COMPLETE").is_file() {
            return Err(format!(
                "{} has no COMPLETE marker: the corpus is missing or was not fetched whole (run `cargo xtask corpus fetch`)",
                dir.display()
            ));
        }
        let docs = documents(&dir);
        if docs.is_empty() {
            return Err(format!("{} holds no documents", dir.display()));
        }
        let index = match std::fs::read_to_string(dir.join("INDEX.tsv")) {
            Ok(text) => Some(parse_index(&text).map_err(|e| format!("{}: {e}", dir.display()))?),
            Err(_) => None,
        };
        if let Some(index) = &index {
            let on_disk: HashSet<&Path> = docs
                .iter()
                .filter_map(|p| p.strip_prefix(&dir).ok())
                .collect();
            let unlisted = on_disk.iter().filter(|p| !index.contains_key(**p)).count();
            let missing = index
                .keys()
                .filter(|p| !on_disk.contains(p.as_path()))
                .count();
            if unlisted + missing > 0 {
                return Err(format!(
                    "{}: {unlisted} documents are not in INDEX.tsv and {missing} files it lists are not there",
                    dir.display()
                ));
            }
        }
        sets.push(CorpusSet { dir, docs, index });
    }
    Ok(sets)
}

fn is_hostile(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "hostile")
}

fn first_diff(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Differences between two packages that the set of changed parts does not explain. Every part
/// outside `changed` must have a raw-identical region and a central record that differs at most in
/// the local header offset field.
pub(super) fn locality_violations(
    before: &Package,
    after: &Package,
    changed: &HashSet<String>,
) -> Vec<String> {
    let a: Vec<&str> = before.part_names().collect();
    let b: Vec<&str> = after.part_names().collect();
    if a.len() > b.len() || a.iter().zip(&b).any(|(x, y)| x != y) {
        return vec![format!(
            "entry order or names differ: {} before, {} after",
            a.len(),
            b.len()
        )];
    }
    let mut out = Vec::new();
    for name in a {
        if changed.contains(name) {
            continue;
        }
        if before.raw_region(name) != after.raw_region(name) {
            out.push(format!("raw bytes of untouched entry {name:?} differ"));
        }
        let (Some(x), Some(y)) = (
            before.raw_central_record(name),
            after.raw_central_record(name),
        ) else {
            out.push(format!("missing central record for {name:?}"));
            continue;
        };
        let mask = |r: &[u8]| {
            let mut v = r.to_vec();
            if let Some(s) = v.get_mut(42..46) {
                s.fill(0);
            }
            v
        };
        // A ZIP64 offset lives in the extra field, so those records are compared by length only.
        let z64 = x.get(42..46) == Some(&[0xFF; 4]);
        if x.len() != y.len() || (!z64 && mask(x) != mask(y)) {
            out.push(format!(
                "central record of untouched entry {name:?} differs beyond its offset"
            ));
        }
    }
    out
}

/// What this crate reads from a package, summarized as `reference_reader` summarizes an archive:
/// the name, size and CRC-32 of every part, in directory order. `None` if some part does not read
/// here: the other reader refuses such an archive as a whole, so there is nothing to compare.
fn summarize(pkg: &Package) -> Option<Vec<reference::Seen>> {
    pkg.part_names()
        .map(|name| {
            let data = pkg.part(name).ok()??;
            Some(reference::Seen {
                name: name.to_owned(),
                len: data.len(),
                crc: reference::crc32(&data),
            })
        })
        .collect()
}

/// Law 5: the other reader opens `bytes` and finds what `ours` says is in it.
fn another_reader_agrees(what: &str, bytes: &[u8], ours: &[reference::Seen]) -> Result<(), String> {
    let theirs: Vec<reference::Seen> = reference::summary(bytes)
        .map_err(|e| format!("{what} opens here, and the other reader refuses it: {e}"))?
        .into_iter()
        .filter(|s| !s.name.ends_with('/'))
        .collect();
    if let Some((o, t)) = ours.iter().zip(&theirs).find(|(o, t)| o != t) {
        return Err(format!(
            "{what} reads differently in the other reader: {o:?} here, {t:?} there"
        ));
    }
    if ours.len() != theirs.len() {
        return Err(format!(
            "{what} has {} parts here and {} in the other reader",
            ours.len(),
            theirs.len()
        ));
    }
    Ok(())
}

/// Checks one valid document. The error says which law failed and where.
fn check_identity(bytes: Vec<u8>) -> Result<(), String> {
    let limits = Limits::default();
    let pkg = Package::open(bytes.clone(), &limits).map_err(|e| format!("open: {e}"))?;
    let before = summarize(&pkg);
    if let Some(ours) = &before {
        another_reader_agrees("the input", &bytes, ours)?;
    }
    let out = pkg.write().map_err(|e| format!("write: {e}"))?;
    if out != bytes {
        return Err(format!(
            "write differs from the input at byte {}",
            first_diff(&out, &bytes)
        ));
    }
    let out = pkg.write_rebuilt().map_err(|e| format!("rebuild: {e}"))?;
    if out != bytes {
        return Err(format!(
            "rebuild differs from the input at byte {}",
            first_diff(&out, &bytes)
        ));
    }
    // Replace every part that reads and is small or XML with its own bytes.
    let mut edited = pkg.clone();
    let mut changed: HashSet<String> = HashSet::new();
    for name in pkg.part_names().map(str::to_owned).collect::<Vec<_>>() {
        if name.ends_with('/') {
            continue;
        }
        if let Ok(Some(part)) = pkg.part(&name)
            && (part.len() <= 256 * 1024 || name.ends_with(".xml") || name.ends_with(".rels"))
        {
            edited
                .replace_part(&name, part.to_vec())
                .map_err(|e| format!("replace {name}: {e}"))?;
            changed.insert(name);
        }
    }
    let rewritten = edited
        .write()
        .map_err(|e| format!("write after replace: {e}"))?;
    let re = Package::open(rewritten.clone(), &limits)
        .map_err(|e| format!("reopen after replace: {e}"))?;
    if let Some(v) = locality_violations(&pkg, &re, &changed).first() {
        return Err(v.clone());
    }
    for name in &changed {
        if pkg.part(name).ok() != re.part(name).ok() {
            return Err(format!(
                "part {name} reads back differently after replacement"
            ));
        }
    }
    if let Some(ours) = &before {
        another_reader_agrees(
            "the archive written after replacing every part",
            &rewritten,
            ours,
        )?;
    }
    check_one_edit(&pkg, before.as_deref())
}

/// L2 smoke: change the text of the first `w:t` in the main part by editing its bytes. `before` is
/// what the other reader has to have seen in the input.
fn check_one_edit(pkg: &Package, before: Option<&[reference::Seen]>) -> Result<(), String> {
    let Some(main) = pkg.main_part().map(str::to_owned) else {
        return Ok(());
    };
    let Ok(Some(data)) = pkg.part(&main) else {
        return Ok(());
    };
    let Some(open) = find(&data, b"<w:t>").or_else(|| find(&data, b"<w:t ")) else {
        return Ok(());
    };
    let Some(gt) = find(data.get(open..).unwrap_or(&[]), b">").map(|i| open + i + 1) else {
        return Ok(());
    };
    let Some(end) = find(data.get(gt..).unwrap_or(&[]), b"</w:t>").map(|i| gt + i) else {
        return Ok(());
    };
    let mut new = data.get(..gt).unwrap_or(&[]).to_vec();
    new.extend_from_slice(b"Edited text");
    new.extend_from_slice(data.get(end..).unwrap_or(&[]));
    let mut edited = pkg.clone();
    edited
        .replace_part(&main, new.clone())
        .map_err(|e| format!("edit {main}: {e}"))?;
    let out = edited
        .write()
        .map_err(|e| format!("write after edit: {e}"))?;
    let re = Package::open(out.clone(), &Limits::default())
        .map_err(|e| format!("reopen after edit: {e}"))?;
    let changed: HashSet<String> = std::iter::once(main.clone()).collect();
    if let Some(v) = locality_violations(pkg, &re, &changed).first() {
        return Err(format!("one-edit locality: {v}"));
    }
    match re.part(&main) {
        Ok(Some(got)) if got.as_slice() == new.as_slice() => {}
        _ => {
            return Err(format!(
                "the edited part {main} does not read back as written"
            ));
        }
    }
    // The other reader sees the edit, and only the edit.
    if let Some(before) = before {
        let seen = reference::summary(&out)
            .map_err(|e| format!("the archive written after one edit opens here, and the other reader refuses it: {e}"))?;
        let names: Vec<&str> = before.iter().map(|s| s.name.as_str()).collect();
        let now: Vec<&str> = seen
            .iter()
            .filter(|s| !s.name.ends_with('/'))
            .map(|s| s.name.as_str())
            .collect();
        if names != now {
            return Err("the other reader finds other parts after one edit".to_owned());
        }
        for (b, s) in before
            .iter()
            .zip(seen.iter().filter(|s| !s.name.ends_with('/')))
        {
            let want = if b.name.eq_ignore_ascii_case(&main) {
                reference::Seen {
                    name: b.name.clone(),
                    len: new.len(),
                    crc: reference::crc32(&new),
                }
            } else {
                reference::Seen {
                    name: b.name.clone(),
                    len: b.len,
                    crc: b.crc,
                }
            };
            if *s != want {
                return Err(format!(
                    "after one edit the other reader finds {s:?}, where {want:?} was expected"
                ));
            }
        }
    }
    Ok(())
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    memchr::memmem::find(hay, needle)
}

/// A hostile document may fail anywhere, with an error, and must not panic.
fn check_clean_failure(bytes: Vec<u8>) {
    if let Ok(pkg) = Package::open(bytes, &Limits::default()) {
        for name in pkg.part_names().map(str::to_owned).collect::<Vec<_>>() {
            let _ = pkg.part(&name);
        }
        let _ = pkg.write();
        let _ = pkg.write_rebuilt();
    }
}

/// Checks one document against what is expected of it. The error says what was wrong.
fn check_document(bytes: Vec<u8>, expect: Option<&Expect>, hostile: bool) -> Result<(), String> {
    match expect {
        Some(e) if e.code == "ok" => check_identity(bytes),
        Some(e) => check_refusal(bytes, e),
        None if hostile => {
            check_clean_failure(bytes);
            Ok(())
        }
        None => check_identity(bytes),
    }
}

/// Reads one document of a set and checks it. A file that cannot be read is a failure, not a skip.
#[cfg(test)]
fn check_path(path: &Path, set: &CorpusSet) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("the file cannot be read: {e}"))?;
    let expect = set
        .index
        .as_ref()
        .and_then(|ix| path.strip_prefix(&set.dir).ok().and_then(|r| ix.get(r)));
    check_document(bytes, expect, is_hostile(path))
}

#[test]
fn laws_hold_on_every_corpus_document() {
    let sets = match load_sets() {
        Ok(Some(sets)) => sets,
        Ok(None) => {
            eprintln!("STETDOC_CORPUS_DIR is not set: skipping the container laws over the corpus");
            return;
        }
        Err(why) => panic!("the corpus cannot be used: {why}"),
    };
    let mut failures: Vec<String> = Vec::new();
    let mut count = 0usize;
    for set in sets {
        count += set.docs.len();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let found = std::sync::Mutex::new(Vec::new());
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(path) = set.docs.get(i) else {
                            break;
                        };
                        let outcome = check_path(path, &set);
                        if let Err(why) = outcome
                            && let Ok(mut f) = found.lock()
                        {
                            f.push(format!("{}: {why}", path.display()));
                        }
                    }
                });
            }
        });
        failures.extend(found.into_inner().unwrap_or_default());
    }
    failures.sort();
    assert!(
        failures.is_empty(),
        "the container laws failed on {} of {count} corpus documents:\n{}",
        failures.len(),
        failures
            .iter()
            .take(120)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod gating;
mod refusal;
