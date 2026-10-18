//! Law L1 for the span tree, over the fetched corpus.
//!
//! The corpus is read from the directories in `STETDOC_CORPUS_DIR`, separated by `:`, with the same
//! rules as `package/corpus_laws.rs`: `sets/curated` (or `sets/<name>` from `STETDOC_CORPUS_SET`)
//! when it exists, else the directory itself; a set that is named has to be whole (a `COMPLETE`
//! marker and at least one document) and agree with its `INDEX.tsv`; a file that cannot be read is a
//! failure. With the variable unset the test skips and passes, and `cargo xtask check` sets
//! the variable for the tests in scope.
//!
//! `xml` may not depend on `package`, so this test reads the parts of each document with a small
//! reader of its own (central directory, ZIP64 records, then raw inflate). That also makes it an
//! independent second reader of the container.
//!
//! For every XML part that parses:
//! 1. `serialize` of the unedited tree returns the input exactly.
//! 2. After every start tag is marked dirty, `serialize` still returns the input exactly. That
//!    proves the dirty path reproduces quotes, spacing, references and tails byte for byte.
//!
//! A part that does not parse must be refused with `malformed_input` or `limit_exceeded`, and must
//! never panic. Beyond that:
//! - For a document that is expected to be good (`expect` is `ok`, or there is no index and the
//!   document is not under `hostile`) the main part must be found, read and parse. So a document
//!   cannot pass by being one this test cannot read.
//! - For a document the index expects to be refused at `xml`, an XML part must be refused with that
//!   code: one of the parts the index's `part` column names, if it names any. A document whose
//!   named parts are none that this test parses (it reads the parts named `.xml` and `.rels`) is not
//!   asked for a refusal. Refusals at the other stages (`open`, `read`) happen before there is
//!   anything to parse, and `document` refusals come from layers above, so those documents are only
//!   held to the rules for parts that do parse.

use super::*;
use crate::api::error::ErrorCode;
use miniz_oxide::inflate::decompress_to_vec_with_limit;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Reads `n` little-endian bytes.
fn le(b: &[u8], o: usize, n: usize) -> Option<usize> {
    let s = b.get(o..o.checked_add(n)?)?;
    usize::try_from(s.iter().rev().fold(0u64, |v, &x| (v << 8) | u64::from(x))).ok()
}

/// Whether this test parses the part: its name ends in `.xml` or `.rels`, in any case.
fn is_xml_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.ends_with(".rels")
}

/// One XML part of a document, as read from the container.
struct Part {
    name: String,
    /// The bytes, or why this reader could not get them.
    bytes: Result<Vec<u8>, String>,
}

/// The XML parts of a ZIP file, by the rules Python's `zipfile` follows: the last end record
/// signature is the end record, and a ZIP64 record right before a locator right before it gives the
/// count, size and offset of the central directory.
fn xml_parts(zip: &[u8]) -> Result<Vec<Part>, String> {
    let eocd = (0..zip.len().saturating_sub(21))
        .rev()
        .find(|&i| zip.get(i..i + 4) == Some(b"PK\x05\x06"))
        .ok_or("no end of central directory record")?;
    let (mut total, mut size, mut offset) = (
        le(zip, eocd + 10, 2),
        le(zip, eocd + 12, 4),
        le(zip, eocd + 16, 4),
    );
    let mut zip64 = false;
    if eocd >= 20 && zip.get(eocd - 20..eocd - 16) == Some(b"PK\x06\x07") {
        let record = eocd.checked_sub(76).ok_or("no room for the ZIP64 record")?;
        if zip.get(record..record + 4) == Some(b"PK\x06\x06") {
            (total, size, offset) = (
                le(zip, record + 32, 8),
                le(zip, record + 40, 8),
                le(zip, record + 48, 8),
            );
            zip64 = true;
        }
    }
    let (Some(total), Some(size), Some(offset)) = (total, size, offset) else {
        return Err("the end record is out of range".into());
    };
    let concat = eocd
        .checked_sub(size + offset + if zip64 { 76 } else { 0 })
        .ok_or("the central directory is not where the end record says")?;
    let mut pos = offset + concat;
    let mut out = Vec::new();
    for _ in 0..total {
        if zip.get(pos..pos + 4) != Some(b"PK\x01\x02") {
            return Err("a central directory record is missing".into());
        }
        let field = |o: usize, n: usize| le(zip, pos + o, n).ok_or("a central record is cut short");
        let (method, nl, el, cl) = (field(10, 2)?, field(28, 2)?, field(30, 2)?, field(32, 2)?);
        let (mut csize, mut usize_, mut header) = (field(20, 4)?, field(24, 4)?, field(42, 4)?);
        let name = String::from_utf8_lossy(
            zip.get(pos + 46..pos + 46 + nl)
                .ok_or("a name is cut short")?,
        )
        .into_owned();
        let extra = zip
            .get(pos + 46 + nl..pos + 46 + nl + el)
            .ok_or("an extra field is cut short")?;
        pos += 46 + nl + el + cl;
        let mut x = 0;
        while x + 4 <= extra.len() {
            let len = le(extra, x + 2, 2).ok_or("bad extra field")?;
            if le(extra, x, 2) == Some(1) {
                let mut o = x + 4;
                for value in [&mut usize_, &mut csize, &mut header] {
                    if *value == 0xFFFF_FFFF {
                        *value = le(extra, o, 8).ok_or("a ZIP64 field is cut short")?;
                        o += 8;
                    }
                }
            }
            x += 4 + len;
        }
        if !is_xml_name(&name) {
            continue;
        }
        let bytes = (|| {
            let lfh = header + concat;
            let (lnl, lel) = (
                le(zip, lfh + 26, 2).ok_or("local header")?,
                le(zip, lfh + 28, 2).ok_or("local header")?,
            );
            let start = lfh + 30 + lnl + lel;
            let data = zip.get(start..start + csize).ok_or("data out of range")?;
            match method {
                0 => Ok(data.to_vec()),
                8 if usize_ <= 64 << 20 => decompress_to_vec_with_limit(data, usize_)
                    .map_err(|e| format!("inflate: {e:?}")),
                8 => Err("too large to check".to_owned()),
                m => Err(format!("compression method {m}")),
            }
        })();
        out.push(Part { name, bytes });
    }
    Ok(out)
}

/// The main part: the part the package relationships call the office document if there is one by
/// that name, else `word/document.xml`. A relationship with no target, or whose target is not in the
/// container, does not hide the conventional name, as in `Package::main_part`.
fn main_part(parts: &[Part]) -> Option<&Part> {
    let by_name = |name: &str| parts.iter().find(|p| p.name.eq_ignore_ascii_case(name));
    let target = parts
        .iter()
        .find(|p| p.name == "_rels/.rels")
        .and_then(|p| p.bytes.as_ref().ok())
        .and_then(|b| {
            SpanTree::parse("_rels/.rels", Bytes::from(b.clone()), &Limits::default()).ok()
        })
        .and_then(|t| {
            t.root()
                .child_elements()
                .find(|r| {
                    r.attr("", "Type")
                        .is_some_and(|ty| ty.ends_with("/officeDocument"))
                })
                .and_then(|r| {
                    r.attr("", "Target")
                        .map(|v| v.trim_start_matches('/').to_owned())
                })
        })
        .filter(|name| !name.is_empty());
    target
        .as_deref()
        .and_then(by_name)
        .or_else(|| by_name("word/document.xml"))
}

/// What `INDEX.tsv` says about one document.
#[derive(Debug)]
struct Expect {
    code: String,
    stage: Option<String>,
    /// The parts that carry a refusal at `xml`, from the `part` column. Empty when it names none.
    parts: Vec<String>,
}

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
            } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                ["docx", "docm", "dotx", "dotm"].contains(&e.to_ascii_lowercase().as_str())
            }) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
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

/// Checks one part that was read. The error names the law that failed. `Ok(Some(code))` says the
/// part was refused with that code, which is allowed for the parts of a document, and is what a
/// refusal at the `xml` stage looks like.
fn check_part(name: &str, bytes: &[u8]) -> Result<Option<ErrorCode>, String> {
    let lim = Limits::default();
    match SpanTree::parse(name, Bytes::from(bytes.to_vec()), &lim) {
        Ok(mut tree) => {
            if tree.serialize() != bytes {
                return Err("an unedited tree does not serialize to its input".into());
            }
            tree.mark_all_tags_dirty();
            if tree.serialize() != bytes {
                return Err("the all-dirty serialization differs from the input".into());
            }
            Ok(None)
        }
        Err(e)
            if matches!(
                e.code(),
                ErrorCode::MalformedInput | ErrorCode::LimitExceeded
            ) =>
        {
            if !e.message().starts_with(&format!("{name}: ")) {
                return Err(format!("an error does not name the part: {e}"));
            }
            Ok(Some(e.code()))
        }
        Err(e) => Err(format!("refused with an unexpected code: {e}")),
    }
}

/// Checks one document. A document is expected to be a good one if the index says `ok` or, with no
/// index, it is not under `hostile`. The main part is required only of a good document outside the
/// hostile tier: that tier holds valid ZIP files of every other kind too, which this layer has no
/// reason to refuse.
fn check_document(zip: &[u8], expect: Option<&Expect>, hostile_dir: bool) -> Result<(), String> {
    let good = match expect {
        Some(e) => e.code == "ok",
        None => !hostile_dir,
    } && !hostile_dir;
    let parts = match xml_parts(zip) {
        Ok(parts) => parts,
        Err(why) if good => return Err(format!("this test cannot read the container: {why}")),
        Err(_) => return Ok(()),
    };
    let mut refused: Vec<(&str, ErrorCode)> = Vec::new();
    for part in &parts {
        if let Ok(bytes) = &part.bytes {
            match check_part(&part.name, bytes) {
                Ok(Some(code)) => refused.push((part.name.as_str(), code)),
                Ok(None) => {}
                Err(why) => return Err(format!("[{}]: {why}", part.name)),
            }
        }
    }
    if good {
        let Some(part) = main_part(&parts) else {
            return Err("the container has no main part".to_owned());
        };
        let main = &part.name;
        let Ok(bytes) = &part.bytes else {
            return Err(format!("the main part {main} cannot be read here"));
        };
        if SpanTree::parse(main, Bytes::from(bytes.clone()), &Limits::default()).is_err() {
            return Err(format!("the main part {main} does not parse"));
        }
    } else if let Some(e) = expect
        && e.stage.as_deref() == Some("xml")
        && (e.parts.is_empty() || e.parts.iter().any(|p| is_xml_name(p)))
        && !refused.iter().any(|(name, code)| {
            code.as_str() == e.code
                && (e.parts.is_empty() || e.parts.iter().any(|p| p.eq_ignore_ascii_case(name)))
        })
    {
        let of = if e.parts.is_empty() {
            String::new()
        } else {
            format!(" of {}", e.parts.join(", "))
        };
        return Err(format!(
            "expected an XML part{of} to be refused with {}, but none was",
            e.code
        ));
    }
    Ok(())
}

/// Reads one document of a set and checks it. A file that cannot be read is a failure.
#[cfg(test)]
fn check_path(path: &Path, set: &CorpusSet) -> Result<(), String> {
    let zip = std::fs::read(path).map_err(|e| format!("the file cannot be read: {e}"))?;
    let expect = set
        .index
        .as_ref()
        .and_then(|ix| path.strip_prefix(&set.dir).ok().and_then(|r| ix.get(r)));
    let hostile = path.components().any(|c| c.as_os_str() == "hostile");
    check_document(&zip, expect, hostile)
}

#[test]
fn laws_hold_for_every_xml_part_of_the_corpus() {
    let Ok(dirs) = std::env::var("STETDOC_CORPUS_DIR") else {
        eprintln!("STETDOC_CORPUS_DIR is not set: skipping the span tree laws over the corpus");
        return;
    };
    let set = std::env::var("STETDOC_CORPUS_SET").unwrap_or_else(|_| "curated".to_owned());
    let sets =
        load_from(&dirs, &set).unwrap_or_else(|why| panic!("the corpus cannot be used: {why}"));
    let mut failures: Vec<String> = Vec::new();
    let mut docs = 0usize;
    for set in &sets {
        docs += set.docs.len();
        for path in &set.docs {
            if let Err(why) = check_path(path, set) {
                failures.push(format!("{}: {why}", path.display()));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the span tree laws failed on {} of {docs} documents:\n{}",
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
mod gating;
