//! LibreOffice as an independent reader of a `.docx`. [`read_all`] drives one headless `soffice` through a Python
//! macro (`libreoffice.py`, run inside soffice with its bundled Python and UNO, so no separate interpreter is needed) and gives a
//! [`Reading`] for each document: whether it loads, whether it converts to PDF, and its text with every tracked change accepted
//! and with every one rejected. It is `None` where LibreOffice is not installed, so a test that uses it skips there.
//!
//! [`compare`] holds a reading against stetdoc's text after the normalization of [`normalize`], which is plain data: the
//! testkit's `laws::libreoffice` makes stetdoc's side from its views, and the core's own tests may use this module too.
//!
//! **Normalization.** Both sides are one line for each paragraph of the body, a table's cells row by row (nested tables in
//! place), and then, apart, the paragraphs of the footnotes and the endnotes. In each line every run of white space (spaces,
//! tabs, line breaks, no-break spaces) is one space, the line is trimmed, and characters that are not text are left out (soft
//! hyphens, zero-width characters, object placeholders); a no-break hyphen is a hyphen. Empty lines are left out. A note's
//! anchor, a comment's anchor and an object are no characters on either side. A field is its result, and because LibreOffice
//! computes some fields again (a page number, a date, a reference), each field and automatic number on stetdoc's side matches
//! any text, and a line that is nothing but fields may also match no line. A page or column break inside a paragraph ends a
//! line on stetdoc's side, as LibreOffice splits the paragraph there. A cell that continues a vertical merge has no lines on
//! stetdoc's side, as Word and LibreOffice draw only the cell that starts the merge (ECMA-376 17.4.85, `ST_Merge`), and so has
//! a note that no reference in the body points to, which neither shows.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The macro that soffice runs.
pub const RUNNER: &str = include_str!("libreoffice.py");

/// The character that stands for a field on stetdoc's side of a comparison: it matches any text.
pub const FIELD: char = '\u{e000}';

/// What LibreOffice did with one document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reading {
    /// The document's path.
    pub path: String,
    /// The error of loading it, or `None` when it loaded.
    pub load: Option<String>,
    /// The error of converting it to PDF, or `None` when it converted (and when it did not load).
    pub pdf: Option<String>,
    /// The body's paragraphs after Accept All, or `None` when it did not get that far.
    pub accept: Option<Vec<String>>,
    /// The body's paragraphs after Reject All.
    pub reject: Option<Vec<String>>,
    /// The notes' paragraphs after Accept All.
    pub notes_accept: Vec<String>,
    /// The notes' paragraphs after Reject All.
    pub notes_reject: Vec<String>,
    /// The tracked changes LibreOffice still had after Accept All.
    pub left_accept: Option<i64>,
    /// The tracked changes LibreOffice still had after Reject All.
    pub left_reject: Option<i64>,
    /// Any other error: soffice crashed or timed out on the document, or a step after loading failed.
    pub error: Option<String>,
}

impl Reading {
    fn from_json(line: &str) -> Result<Reading, String> {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("a result line is not JSON: {e}"))?;
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(String::from);
        let lines = |k: &str| {
            v.get(k).and_then(|x| x.as_array()).map(|a| {
                a.iter()
                    .map(|l| l.as_str().unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
            })
        };
        Ok(Reading {
            path: s("path").unwrap_or_default(),
            load: s("load"),
            pdf: s("pdf"),
            accept: lines("accept"),
            reject: lines("reject"),
            notes_accept: lines("notes_accept").unwrap_or_default(),
            notes_reject: lines("notes_reject").unwrap_or_default(),
            left_accept: v.get("left_accept").and_then(|x| x.as_i64()),
            left_reject: v.get("left_reject").and_then(|x| x.as_i64()),
            error: s("error"),
        })
    }

    fn failed(path: &Path, why: &str) -> String {
        serde_json::json!({ "path": path.display().to_string(), "error": why }).to_string()
    }
}

/// The `soffice` to run: `STETDOC_SOFFICE`, else `soffice` on the `PATH`, else the usual install places. `None` when there is
/// none that answers `--version`.
pub fn soffice() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = std::env::var("STETDOC_SOFFICE") {
        candidates.push(PathBuf::from(p));
    }
    candidates.push(PathBuf::from("soffice"));
    candidates.push(PathBuf::from(
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ));
    candidates.push(PathBuf::from("/usr/bin/soffice"));
    candidates.push(PathBuf::from("/usr/lib/libreoffice/program/soffice"));
    candidates.push(PathBuf::from(
        "C:\\Program Files\\LibreOffice\\program\\soffice.exe",
    ));
    candidates.into_iter().find(|c| {
        Command::new(c)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// How long one document may take before soffice is stopped and the document recorded as timed out: `STETDOC_LO_TIMEOUT`
/// seconds, 120 by default.
fn timeout() -> Duration {
    let secs = std::env::var("STETDOC_LO_TIMEOUT")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(120);
    Duration::from_secs(secs)
}

fn lines_in(path: &Path) -> usize {
    std::fs::File::open(path)
        .map(|f| {
            BufReader::new(f)
                .lines()
                .map_while(Result::ok)
                .filter(|l| !l.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

fn start(soffice: &Path, dir: &Path) -> Result<Child, String> {
    let profile = dir.join("profile");
    let url = file_url(&profile);
    Command::new(soffice)
        .args([
            "--headless",
            "--invisible",
            "--nologo",
            "--norestore",
            "--nolockcheck",
            &format!("-env:UserInstallation={url}"),
            "vnd.sun.star.script:stetdoc_oracle.py$run?language=Python&location=user",
        ])
        .env("STETDOC_LO_JOBS", dir.join("jobs.txt"))
        .env("STETDOC_LO_RESULTS", dir.join("results.jsonl"))
        .env("STETDOC_LO_WORK", dir.join("work"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", soffice.display()))
}

/// The `file:` URL of a path: every byte of its UTF-8 that is not an unreserved character, a `/` or a drive's `:` is
/// percent-encoded, so that a space, `#`, `?`, `%` or a non-ASCII letter in the path cannot change what soffice opens.
fn file_url(path: &Path) -> String {
    let s = path.display().to_string().replace('\\', "/");
    let mut encoded = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'/' | b':') {
            encoded.push(char::from(b));
        } else {
            encoded.push_str(&format!("%{b:02X}"));
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// Reads every document with LibreOffice, in order. `None` when LibreOffice is not installed; `Err` when soffice cannot be made
/// to run the macro at all. A document on which soffice crashes or hangs gets a reading with `error` set, and soffice is started
/// again for the rest.
pub fn read_all(docs: &[PathBuf]) -> Option<Result<Vec<Reading>, String>> {
    let jobs: Vec<(PathBuf, Option<Variant>)> = docs.iter().map(|d| (d.clone(), None)).collect();
    read_variants(&jobs)
}

/// A copy of a document with one feature taken out, which the macro makes before it reads the document, to find out what a
/// mismatch depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// Every revision gets a date of its own, five minutes apart: LibreOffice joins adjacent revisions of one author whose dates
    /// are a minute or less apart into one.
    UniqueDates,
    /// The start and the end of every comment's range are taken out.
    NoCommentRanges,
    /// The run of a note's own number (`w:footnoteRef`, `w:endnoteRef`) is moved out of a deletion that starts with it.
    NoteMarksKept,
}

impl Variant {
    /// The variant's name, as the macro reads it.
    pub fn name(self) -> &'static str {
        match self {
            Variant::UniqueDates => "unique-dates",
            Variant::NoCommentRanges => "no-comment-ranges",
            Variant::NoteMarksKept => "note-marks-kept",
        }
    }
}

/// Like [`read_all`], with a variant of each document where one is given.
pub fn read_variants(docs: &[(PathBuf, Option<Variant>)]) -> Option<Result<Vec<Reading>, String>> {
    let soffice = soffice()?;
    Some(read_with(&soffice, docs))
}

fn read_with(soffice: &Path, docs: &[(PathBuf, Option<Variant>)]) -> Result<Vec<Reading>, String> {
    // A fresh directory of this run's alone, made with a name no other process can have chosen first (the profile holds the
    // macro soffice runs), and removed when it goes out of scope.
    let scratch = tempfile::Builder::new()
        .prefix("stetdoc-lo-")
        .tempdir()
        .map_err(|e| format!("cannot make a scratch directory: {e}"))?;
    let dir = scratch.path().to_path_buf();
    let scripts = dir.join("profile/user/Scripts/python");
    std::fs::create_dir_all(&scripts).map_err(|e| format!("{}: {e}", scripts.display()))?;
    std::fs::create_dir_all(dir.join("work")).map_err(|e| e.to_string())?;
    std::fs::write(scripts.join("stetdoc_oracle.py"), RUNNER).map_err(|e| e.to_string())?;
    let jobs: Vec<String> = docs
        .iter()
        .map(|(d, v)| {
            let p = std::path::absolute(d)
                .unwrap_or_else(|_| d.clone())
                .display()
                .to_string();
            match v {
                Some(v) => format!("{p}\t{}", v.name()),
                None => p,
            }
        })
        .collect();
    std::fs::write(dir.join("jobs.txt"), jobs.join("\n") + "\n").map_err(|e| e.to_string())?;
    let results = dir.join("results.jsonl");
    let out = drive(soffice, &dir, docs, &results);
    let read = out.and_then(|()| {
        let text = std::fs::read_to_string(&results).map_err(|e| e.to_string())?;
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(Reading::from_json)
            .collect::<Result<Vec<_>, _>>()
    });
    drop(scratch);
    read
}

fn append(results: &Path, line: &str) -> Result<(), String> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(results)
        .map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

fn drive(
    soffice: &Path,
    dir: &Path,
    docs: &[(PathBuf, Option<Variant>)],
    results: &Path,
) -> Result<(), String> {
    let limit = timeout();
    // The first start makes the profile, which takes longer than a document.
    let startup = Duration::from_secs(120);
    let mut fruitless = 0;
    while lines_in(results) < docs.len() {
        let mut child = start(soffice, dir)?;
        let began = lines_in(results);
        let mut done = began;
        let mut last = Instant::now();
        let mut grace = startup;
        let stuck = loop {
            std::thread::sleep(Duration::from_millis(100));
            let now = lines_in(results);
            if now != done {
                done = now;
                last = Instant::now();
                grace = Duration::ZERO;
            }
            if let Ok(Some(_)) = child.try_wait() {
                break done < docs.len();
            }
            if last.elapsed() > limit + grace {
                let _ = child.kill();
                let _ = child.wait();
                break true;
            }
        };
        let done = lines_in(results);
        if !stuck || done >= docs.len() {
            continue;
        }
        if done == began {
            fruitless += 1;
            if fruitless >= 3 {
                return Err(format!(
                    "soffice ({}) ran three times without reading a document: it may not run Python macros",
                    soffice.display()
                ));
            }
        } else {
            fruitless = 0;
        }
        if let Some((doc, _)) = docs.get(done) {
            append(
                results,
                &Reading::failed(doc, "soffice exited or timed out on this document"),
            )?;
        }
    }
    Ok(())
}

/// A line, normalized: every run of white space one space, trimmed, without the characters that are not text.
pub fn normalize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut space = false;
    for c in line.chars() {
        let c = match c {
            '\u{ad}'
            | '\u{200b}'
            | '\u{200c}'
            | '\u{200d}'
            | '\u{2060}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffc}' => continue,
            '\u{2011}' => '-',
            c => c,
        };
        if c.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(c);
    }
    out
}

/// Lines normalized, without the empty ones.
pub fn normalize_all(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| normalize(l))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Whether `text` matches `pattern`, in which each [`FIELD`] matches any text (white space around it included).
pub fn matches(pattern: &str, text: &str) -> bool {
    let all: Vec<char> = pattern.chars().collect();
    let p: Vec<char> = all
        .iter()
        .enumerate()
        .filter(|(k, c)| {
            **c != ' '
                || !(k.checked_sub(1).and_then(|b| all.get(b)) == Some(&FIELD)
                    || all.get(k + 1) == Some(&FIELD))
        })
        .map(|(_, c)| *c)
        .collect();
    let t: Vec<char> = text.chars().collect();
    // A glob match with one wildcard character, without backtracking past the last wildcard.
    let (mut i, mut j) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while j < t.len() {
        match p.get(i) {
            Some(&FIELD) => {
                star = Some((i, j));
                i += 1;
            }
            Some(c) if *c == t[j] => {
                i += 1;
                j += 1;
            }
            _ => match star {
                Some((si, sj)) => {
                    i = si + 1;
                    j = sj + 1;
                    star = Some((si, sj + 1));
                }
                None => return false,
            },
        }
    }
    p[i..].iter().all(|c| *c == FIELD)
}

/// Holds stetdoc's lines (with [`FIELD`] for each field, and [`OPAQUE`] for content its view collapses) against LibreOffice's,
/// both normalized. `Err` names the first line that differs, with both texts around the first character that differs.
pub fn compare(stetdoc: &[String], lo: &[String]) -> Result<(), String> {
    let mut failed = std::collections::HashSet::new();
    let mut furthest = (0, 0);
    if aligns(stetdoc, lo, 0, 0, &mut failed, &mut furthest) {
        return Ok(());
    }
    // Report where the alignment got furthest before it failed.
    let (i, j) = furthest;
    Err(match (stetdoc.get(i), lo.get(j)) {
        (Some(s), Some(l)) => format!("line {i}: {}", difference(s, l)),
        (Some(s), None) => format!(
            "line {i}: LibreOffice has no more lines, stetdoc {:?}",
            shown(s, 0)
        ),
        (None, Some(l)) => format!(
            "line {i}: stetdoc has no more lines, LibreOffice {:?}",
            shown(l, 0)
        ),
        (None, None) => format!("line {i}: the lines do not line up"),
    })
}

/// Whether `stetdoc[i..]` lines up with `lo[j..]`. A line of fields alone may stand for one line or none, and an opaque line
/// for any number; each choice is tried, and a pair of positions that failed is not tried again.
fn aligns(
    stetdoc: &[String],
    lo: &[String],
    i: usize,
    j: usize,
    failed: &mut std::collections::HashSet<(usize, usize)>,
    furthest: &mut (usize, usize),
) -> bool {
    let mut i = i;
    let mut j = j;
    loop {
        if (i, j) > *furthest {
            *furthest = (i, j);
        }
        let Some(s) = stetdoc.get(i) else {
            return j == lo.len();
        };
        let opaque = s == OPAQUE_LINE;
        let only_fields = !opaque && s.chars().all(|c| c == FIELD || c == ' ');
        if !opaque && !only_fields {
            // A plain line: the common case, without recursion.
            match lo.get(j) {
                Some(l) if matches(s, l) => {
                    i += 1;
                    j += 1;
                    continue;
                }
                _ => return false,
            }
        }
        if failed.contains(&(i, j)) {
            return false;
        }
        let ok = if opaque {
            (j..=lo.len()).any(|k| aligns(stetdoc, lo, i + 1, k, failed, furthest))
        } else {
            (lo.get(j).is_some_and(|l| matches(s, l))
                && aligns(stetdoc, lo, i + 1, j + 1, failed, furthest))
                || aligns(stetdoc, lo, i + 1, j, failed, furthest)
        };
        if !ok {
            failed.insert((i, j));
        }
        return ok;
    }
}

/// The character that marks content stetdoc's view collapses into one opaque block, such as a table of contents.
pub const OPAQUE: char = '\u{e001}';
const OPAQUE_LINE: &str = "\u{e001}";

/// Two lines around the first character that differs (a field counts as one character of stetdoc's).
fn difference(s: &str, l: &str) -> String {
    let at = s
        .chars()
        .zip(l.chars())
        .position(|(a, b)| a != b && a != FIELD)
        .unwrap_or(s.chars().count().min(l.chars().count()));
    format!(
        "at character {at}: stetdoc {:?}, LibreOffice {:?}",
        shown(s, at),
        shown(l, at)
    )
}

/// A line as a mismatch shows it: a field as `{field}`, and 120 characters from 40 before `at`.
fn shown(line: &str, at: usize) -> String {
    let from = at.saturating_sub(40);
    let n = line.chars().count();
    let part: String = line.chars().skip(from).take(120).collect();
    let part = part.replace(FIELD, "{field}").replace(OPAQUE, "{opaque}");
    format!(
        "{}{part}{}",
        if from > 0 { "..." } else { "" },
        if from + 120 < n { "..." } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_file_url_encodes_every_character_a_url_cannot_hold() {
        assert_eq!(
            file_url(Path::new("/tmp/a b/c#d?e%f/é.txt")),
            "file:///tmp/a%20b/c%23d%3Fe%25f/%C3%A9.txt"
        );
        assert_eq!(file_url(Path::new("C:\\x y\\p")), "file:///C:/x%20y/p");
    }

    #[test]
    fn normalizing_collapses_white_space_and_leaves_out_what_is_not_text() {
        assert_eq!(normalize("  a\t b\u{a0}\u{a0}c \n"), "a b c");
        assert_eq!(
            normalize("co\u{ad}op\u{200b}\u{fffc} non\u{2011}stop"),
            "coop non-stop"
        );
        assert_eq!(normalize_all(&v(&["", " ", "x"])), v(&["x"]));
    }

    #[test]
    fn a_field_matches_any_text_and_nothing_else_does() {
        let f = FIELD.to_string();
        assert!(matches(&format!("Page {f} of {f}"), "Page 3 of 12"));
        assert!(matches(&format!("See {f}."), "See clause 4.2."));
        assert!(matches(&format!("a{f}"), "a"));
        assert!(!matches("Page 3", "Page 4"));
        assert!(!matches(&format!("a {f} b"), "a x c"));
        assert!(matches("a b", "a b"));
        assert!(!matches("ab", "a b"));
        assert!(matches(&format!("{f} Title"), "Title"));
    }

    #[test]
    fn a_comparison_names_the_first_line_that_differs() {
        let f = FIELD.to_string();
        assert!(compare(&v(&["one", &f, "two"]), &v(&["one", "two"])).is_ok());
        assert!(compare(&v(&["one", &f, "two"]), &v(&["one", "7", "two"])).is_ok());
        let e = compare(&v(&["one", "two"]), &v(&["one", "too"])).unwrap_err();
        assert!(e.contains("line 1") && e.contains("too"), "{e}");
        let e = compare(&v(&["one"]), &v(&["one", "more"])).unwrap_err();
        let o = OPAQUE.to_string();
        // A line of fields that stands for a line of LibreOffice's (a table of contents), even where the next line would match.
        let toc = v(&["Contents", &f, "1. Terms", "Body"]);
        assert!(compare(&toc, &v(&["Contents", "1. Terms", "1. Terms", "Body"])).is_ok());
        assert!(compare(&v(&["a", &o, "b"]), &v(&["a", "x", "y", "b"])).is_ok());
        assert!(compare(&v(&["a", &o]), &v(&["a", "x"])).is_ok());
        assert!(e.contains("no more lines"), "{e}");
        assert!(compare(&v(&["one", "two"]), &v(&["one"])).is_err());
    }

    #[test]
    fn a_missing_libreoffice_is_none_and_a_present_one_reads_a_document() {
        let docx = crate::fixtures::zip::Zip::new(crate::fixtures::parts::entries()).build();
        let path =
            std::env::temp_dir().join(format!("stetdoc-lo-unit-{}.docx", std::process::id()));
        std::fs::write(&path, docx).unwrap();
        let read = read_all(std::slice::from_ref(&path));
        let _ = std::fs::remove_file(&path);
        let Some(read) = read else {
            eprintln!("skipped: LibreOffice is not installed");
            return;
        };
        let read = read.unwrap();
        assert_eq!(read.len(), 1);
        let r = &read[0];
        assert_eq!((&r.load, &r.pdf, &r.error), (&None, &None, &None), "{r:?}");
        let text = r.accept.clone().unwrap().join("\n");
        assert!(text.contains("Hello edge"), "{text:?}");
    }
}
