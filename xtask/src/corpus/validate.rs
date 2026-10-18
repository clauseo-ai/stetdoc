//! The static check of the corpus manifest, run by `cargo xtask check` and `cargo xtask corpus
//! check`: the files are well formed, every source is described in `SOURCES.md`, every refusal
//! has a stage and a reason from the vocabulary, the refusal codes are `ErrorCode` wire names,
//! every compare pair names two distinct documents of the manifest that must open, and no
//! document is committed.

use super::manifest::{
    DIR, Doc, How, Manifest, NONE, PAIR_CLASSES, REFUSAL_CODES, STAGES, TIERS, Texts, hex_sha256,
};
use crate::enumspec::ENUM_FILE;
use crate::repo::{Repo, Res};
use crate::violation::Violation;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Extensions of files that must never be committed: the repository commits no
/// third-party document, and fixtures come from the testkit fixture builder.
const DOCUMENT_EXTENSIONS: &[&str] = &["docx", "docm", "dotx", "dotm"];

/// The `ErrorCode` wire names, read from the core's `error.rs` without compiling it.
fn wire_names_of(error_rs: &str) -> Res<BTreeSet<String>> {
    let specs = crate::enumspec::parse(error_rs).map_err(|v| v.to_string())?;
    Ok(specs.codes.into_iter().map(|c| c.wire).collect())
}

/// Lists the committed files that look like documents.
fn committed_documents(repo: &Repo) -> Res<Vec<Violation>> {
    let mut found = Vec::new();
    for file in repo.files()? {
        let ext = file.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        if DOCUMENT_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
            found.push(Violation::new(
                &file,
                0,
                "a document is committed",
                "the repository commits no third-party document: add it to corpus/manifest/ with its pinned source, or build it with the testkit fixture builder",
            ));
        }
    }
    Ok(found)
}

/// The static check: the manifest is well formed, every source has a `SOURCES.md` entry,
/// every refusal has a stage and a reason, and no document is committed. It always validates
/// every row, and leaves a stamp that a pass was recorded (see [`check_cached`]).
pub fn check(repo: &Repo) -> Res<Vec<Violation>> {
    static_check(repo, false)
}

/// The same check for `cargo xtask check --quick`: the rows of the manifest (11 MB, most of it
/// Harvey LAB) are validated again only when a manifest file, `SOURCES.md`, the error codes or
/// this binary changed since the last pass. The scan for committed documents always runs.
pub fn check_cached(repo: &Repo) -> Res<Vec<Violation>> {
    static_check(repo, true)
}

/// The manifest part of the check alone, skipping rows that already passed. `corpus fetch`
/// runs it first, because the tier and the source name end up in file paths.
pub fn check_manifest_cached(repo: &Repo) -> Res<Vec<Violation>> {
    manifest_check(repo, true)
}

fn static_check(repo: &Repo, cached: bool) -> Res<Vec<Violation>> {
    let mut found = committed_documents(repo)?;
    if !repo.exists(&format!("{DIR}/sources.tsv")) {
        found.push(Violation::new(
            format!("{DIR}/sources.tsv"),
            0,
            "the corpus sources file is missing",
            "restore corpus/sources.tsv",
        ));
        return Ok(found);
    }
    found.extend(manifest_check(repo, cached)?);
    Ok(found)
}

fn manifest_check(repo: &Repo, cached: bool) -> Res<Vec<Violation>> {
    let texts = Texts::read(repo)?;
    let sources_md = repo.read(&format!("{DIR}/SOURCES.md")).unwrap_or_default();
    let error_rs = repo.read(ENUM_FILE)?;
    let stamp = Stamp::new(repo, &texts, &sources_md, &error_rs);
    if cached && stamp.is_current() {
        return Ok(Vec::new());
    }
    let manifest = Manifest::parse(&texts)?;
    let violations = check_manifest(&manifest, &sources_md, &wire_names_of(&error_rs)?);
    if violations.is_empty() {
        stamp.write();
    }
    Ok(violations)
}

/// A record that the manifest passed the check, kept under `target/` and keyed by everything
/// the check reads. It is only a speed-up: a missing, stale or unwritable stamp means the full
/// check runs.
struct Stamp {
    path: PathBuf,
    key: String,
}

impl Stamp {
    fn new(repo: &Repo, texts: &Texts, sources_md: &str, error_rs: &str) -> Stamp {
        let mut key = texts.hash();
        for part in [&texts.reasons, sources_md, error_rs] {
            key.push('\n');
            key.push_str(part);
        }
        // A rebuilt xtask may check more than the last one did.
        if let Ok(exe) = std::env::current_exe()
            && let Ok(meta) = std::fs::metadata(exe)
        {
            key.push_str(&format!("\n{}:{:?}", meta.len(), meta.modified().ok()));
        }
        Stamp {
            path: repo
                .root()
                .join("target")
                .join("xtask")
                .join("corpus-manifest.ok"),
            key: hex_sha256(key.as_bytes()),
        }
    }

    fn is_current(&self) -> bool {
        std::fs::read_to_string(&self.path).is_ok_and(|t| t.trim() == self.key)
    }

    fn write(&self) {
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.path, &self.key);
    }
}

/// A refusal at the `read` or `xml` stage names the parts that carry it, because a layer that
/// never reads them does not meet it. Any other refusal has no part.
fn check_part(d: &Doc) -> Vec<Violation> {
    let (file, line) = (&d.origin.0, d.origin.1);
    let carried = matches!(d.stage.as_str(), "read" | "xml");
    if !carried {
        if d.part == NONE {
            return Vec::new();
        }
        return vec![Violation::new(
            file,
            line,
            format!("part `{}` on a refusal at the `{}` stage", d.part, d.stage),
            format!("write `{NONE}`: only a refusal at the read or xml stage is carried by a part"),
        )];
    }
    if d.part == NONE || d.part.is_empty() {
        return vec![Violation::new(
            file,
            line,
            format!("a refusal at the `{}` stage names no part", d.stage),
            "name the part that carries it, such as word/document.xml; several are separated by `;`",
        )];
    }
    d.part
        .split(';')
        .filter(|p| p.is_empty() || p.starts_with('/') || p.chars().any(char::is_control))
        .map(|p| {
            Violation::new(
                file,
                line,
                format!("`{p}` is not a part name"),
                "write part names as they appear in the archive, without a leading slash",
            )
        })
        .collect()
}

/// Checks a parsed manifest against the text of `SOURCES.md` and the `ErrorCode` wire names.
pub fn check_manifest(m: &Manifest, sources_md: &str, wires: &BTreeSet<String>) -> Vec<Violation> {
    let mut v = Vec::new();
    let sources_file = format!("{DIR}/sources.tsv");
    let reasons_file = format!("{DIR}/reasons.tsv");
    for code in REFUSAL_CODES {
        if !wires.contains(*code) {
            v.push(Violation::new(
                &reasons_file,
                0,
                format!("the refusal code `{code}` is not an ErrorCode wire name"),
                "keep REFUSAL_CODES in xtask/src/corpus/manifest.rs in step with the error_codes! list in api/error.rs",
            ));
        }
    }
    let mut vocabulary: BTreeSet<&str> = BTreeSet::new();
    for r in &m.reasons {
        if !vocabulary.insert(r.name.as_str()) {
            v.push(Violation::new(
                &reasons_file,
                r.line,
                format!("reason `{}` is listed twice", r.name),
                "keep one row per reason",
            ));
        }
        if r.meaning.trim().is_empty() {
            v.push(Violation::new(
                &reasons_file,
                r.line,
                format!("reason `{}` has no meaning", r.name),
                "say in a sentence what the reason means",
            ));
        }
    }
    let mut names = BTreeSet::new();
    for s in &m.sources {
        if !names.insert(s.name.as_str()) {
            v.push(Violation::new(
                &sources_file,
                s.line,
                format!("source `{}` is listed twice", s.name),
                "keep one row per source",
            ));
        }
        if s.how == How::Fetch && !is_hex(&s.commit, 40) {
            v.push(Violation::new(
                &sources_file,
                s.line,
                format!(
                    "source `{}` has no pinned commit (40 lower-case hex digits)",
                    s.name
                ),
                "pin the commit the documents were taken from",
            ));
        }
        if s.spdx.is_empty() {
            v.push(Violation::new(
                &sources_file,
                s.line,
                format!("source `{}` has no licence", s.name),
                "record the SPDX identifier of the source",
            ));
        }
        if !sources_md.contains(&format!("| {} |", s.name)) {
            v.push(Violation::new(
                format!("{DIR}/SOURCES.md"),
                0,
                format!("source `{}` has no entry", s.name),
                format!("add a table row starting `| {} |` with its URL, commit, licence and what we take", s.name),
            ));
        }
    }
    let mut seen: BTreeMap<&str, &Doc> = BTreeMap::new();
    for d in &m.docs {
        let (file, line) = (&d.origin.0, d.origin.1);
        if m.source(&d.source).is_none() {
            v.push(Violation::new(
                file,
                line,
                format!("source `{}` is not in sources.tsv", d.source),
                "add it to corpus/sources.tsv",
            ));
        }
        if !is_hex(&d.sha256, 64) {
            v.push(Violation::new(
                file,
                line,
                "the SHA-256 is not 64 lower-case hex digits",
                "write the hash of the file",
            ));
        }
        if !TIERS.contains(&d.tier.as_str()) {
            v.push(Violation::new(
                file,
                line,
                format!("unknown tier `{}`", d.tier),
                format!("use one of {}", TIERS.join(", ")),
            ));
        }
        if d.expect == "ok" {
            if d.stage != NONE {
                v.push(Violation::new(
                    file,
                    line,
                    format!("stage `{}` on a document that must open", d.stage),
                    format!("write `{NONE}` as the stage of an `ok` document"),
                ));
            }
            if d.part != NONE {
                v.push(Violation::new(
                    file,
                    line,
                    format!("part `{}` on a document that must open", d.part),
                    format!("write `{NONE}` as the part of an `ok` document"),
                ));
            }
        } else if !REFUSAL_CODES.contains(&d.expect.as_str()) || !wires.contains(&d.expect) {
            v.push(Violation::new(
                file,
                line,
                format!("unknown expected outcome `{}`", d.expect),
                format!("use ok or one of {}", REFUSAL_CODES.join(", ")),
            ));
        } else {
            if d.tier != "hostile" {
                v.push(Violation::new(
                    file,
                    line,
                    format!("expected outcome `{}` outside the hostile tier", d.expect),
                    "a document stetdoc refuses belongs to the hostile tier",
                ));
            }
            if !STAGES.contains(&d.stage.as_str()) {
                v.push(Violation::new(
                    file,
                    line,
                    format!("the refusal has stage `{}`", d.stage),
                    format!("say where it is refused: one of {}", STAGES.join(", ")),
                ));
            }
            v.extend(check_part(d));
        }
        let needs_reason = d.tier == "hostile" || d.expect != "ok";
        if d.reason == NONE || d.reason.is_empty() {
            if needs_reason {
                v.push(Violation::new(
                    file,
                    line,
                    "a hostile or refused document has no reason",
                    "list why in the vocabulary of corpus/reasons.tsv, separated by `;`",
                ));
            }
        } else {
            for token in d.reason.split(';') {
                if !vocabulary.contains(token) {
                    v.push(Violation::new(
                        file,
                        line,
                        format!("reason `{token}` is not in corpus/reasons.tsv"),
                        "use a reason from the vocabulary, or add it there with its meaning",
                    ));
                }
            }
        }
        if !matches!(d.word_fp.as_str(), "y" | "n" | "?") {
            v.push(Violation::new(
                file,
                line,
                format!("word_fp `{}` is not y, n or ?", d.word_fp),
                "write y, n or ? (not scanned)",
            ));
        }
        if d.spdx.is_empty() {
            v.push(Violation::new(
                file,
                line,
                "the licence is empty",
                "record the SPDX identifier",
            ));
        }
        if let Some(first) = seen.insert(d.sha256.as_str(), d) {
            v.push(Violation::new(
                file,
                line,
                format!(
                    "the same hash is already listed at {}:{}",
                    first.origin.0, first.origin.1
                ),
                "list each distinct file once",
            ));
        }
    }
    for c in &m.curated {
        let file = format!("{DIR}/curated.tsv");
        if c.reason.trim().is_empty() {
            v.push(Violation::new(
                &file,
                c.line,
                format!("`{}` has no reason", c.id),
                "say what the document adds to the per-pull-request corpus",
            ));
        }
        match seen.get(c.sha256.as_str()) {
            None => v.push(Violation::new(
                &file,
                c.line,
                format!("`{}` is not in the manifest", c.id),
                "curated documents must be listed in corpus/manifest/ first",
            )),
            Some(d) if d.source != c.source => v.push(Violation::new(
                &file,
                c.line,
                format!(
                    "`{}` belongs to source `{}`, not `{}`",
                    c.id, d.source, c.source
                ),
                "fix the source column",
            )),
            Some(_) => {}
        }
    }
    check_pairs(m, &seen, &mut v);
    v
}

/// `pairs.tsv`: a known class, two different documents of the manifest that must open, each
/// pair once in either order, a label, and an outlier reason that says something.
fn check_pairs(m: &Manifest, seen: &BTreeMap<&str, &Doc>, v: &mut Vec<Violation>) {
    let file = format!("{DIR}/pairs.tsv");
    let mut pairs: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for p in &m.pairs {
        let mut bad = |message: String, fix: &str| {
            v.push(Violation::new(&file, p.line, message, fix));
        };
        if !PAIR_CLASSES.contains(&p.class.as_str()) {
            bad(
                format!("class `{}` is not one of {PAIR_CLASSES:?}", p.class),
                "use a class of docs/design/compare.md, section 1.2",
            );
        }
        if p.label.trim().is_empty() || p.outlier.trim().is_empty() {
            bad(
                format!(
                    "the pair at line {} has an empty label or outlier column",
                    p.line
                ),
                "give a label, and an outlier reason or `-`",
            );
        }
        for sha in [&p.original, &p.revised] {
            match seen.get(sha.as_str()) {
                None => bad(
                    format!("`{sha}` of pair `{}` is not in the manifest", p.label),
                    "list the document in corpus/manifest/ first",
                ),
                Some(d) if d.expect != "ok" => bad(
                    format!("`{sha}` of pair `{}` is refused (`{}`)", p.label, d.expect),
                    "a pair is two documents that open",
                ),
                Some(_) => {}
            }
        }
        if p.original == p.revised {
            bad(
                format!("pair `{}` compares a document with itself", p.label),
                "list two different documents",
            );
        }
        let key = if p.original <= p.revised {
            (p.original.as_str(), p.revised.as_str())
        } else {
            (p.revised.as_str(), p.original.as_str())
        };
        if let Some(first) = pairs.insert(key, p.line) {
            bad(
                format!("pair `{}` is already listed at line {first}", p.label),
                "list each pair once",
            );
        }
    }
}

#[cfg(test)]
mod tests;
