//! The corpus manifest: which documents exist, where each one comes from and what stetdoc
//! must do with it.
//!
//! The manifest is tab-separated text under `corpus/`, never a document:
//!
//! - `sources.tsv`: `name, url, commit, spdx, how, vendorable` per source
//! - `manifest/<source>.tsv`: `sha256, size, tier, expect, stage, part, reason, spdx, producer,
//!   word_fp, path` per document
//! - `curated.tsv`: `sha256, source, id, reason` per document of the per-pull-request corpus
//! - `reasons.tsv`: `reason, meaning` for every reason a document is hostile or refused
//! - `pairs.tsv`: `class, set, original, revised, label, outlier` per compare pair;
//!   the documents of the `curated` pairs are fetched with the per-pull-request corpus into a set of
//!   their own, `curatedpairs`, so they do not change the documents the other corpus tests sample
//!
//! `expect` is `ok` or the error code stetdoc must refuse the document with, and `stage` says
//! where in the pipeline that happens. A refusal at the `read` or `xml` stage is carried by
//! one or more parts, named in `part`: a layer that never reads those parts does not meet the
//! refusal, and for it the document must open. The refusals are the exemption list of law L1,
//! so each one carries a `reason` in the vocabulary of `reasons.tsv`.
//!
//! Lines starting with `#` and blank lines are ignored.

use crate::repo::{Repo, Res};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Where the manifest lives, relative to the repository root.
pub const DIR: &str = "corpus";

/// The tiers a corpus document can belong to.
pub const TIERS: &[&str] = &[
    "lab",
    "word-authored",
    "other-producers",
    "public",
    "edge",
    "hostile",
];

/// The error codes stetdoc may refuse a document with. Each must be an `ErrorCode` wire name.
pub const REFUSAL_CODES: &[&str] = &["malformed_input", "limit_exceeded", "unsupported"];

/// Where a refusal happens, in pipeline order: opening the ZIP, reading a part, parsing a
/// part as XML, and the document layer (the package is not a usable `.docx`).
pub const STAGES: &[&str] = &["open", "read", "xml", "document"];

/// The `stage`, `part` and `reason` of a row that has none.
pub const NONE: &str = "-";

/// How a source's documents are obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// Downloaded from the source at the pinned commit.
    Fetch,
    /// Built in tests by the testkit fixture builder. Nothing is downloaded.
    Generate,
    /// Our own output of a third-party tool, with no download location yet.
    Host,
}

/// One upstream repository, pinned to a commit.
#[derive(Clone, Debug)]
pub struct Source {
    /// Short name, also the manifest file name.
    pub name: String,
    /// Clone URL.
    pub url: String,
    /// The pinned commit (40 hex digits) for `Fetch` sources.
    pub commit: String,
    /// Licence of the source.
    pub spdx: String,
    /// How its documents are obtained.
    pub how: How,
    /// Line in `sources.tsv`.
    pub line: usize,
}

/// One document.
#[derive(Clone, Debug)]
pub struct Doc {
    /// Lower-case SHA-256 of the file.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// One of [`TIERS`].
    pub tier: String,
    /// `ok`, or one of [`REFUSAL_CODES`].
    pub expect: String,
    /// One of [`STAGES`] for a refusal, [`NONE`] for `ok`.
    pub stage: String,
    /// The parts that carry a refusal at the `read` or `xml` stage, separated by `;`, and
    /// [`NONE`] for every other row.
    pub part: String,
    /// Why the document is hostile or refused: `;`-separated entries of `reasons.tsv`, or
    /// [`NONE`] for an ordinary document.
    pub reason: String,
    /// Licence of this document.
    pub spdx: String,
    /// Producer family read from the document properties.
    pub producer: String,
    /// Whether the document looks written by real Word: `y`, `n`, or `?` when not scanned.
    pub word_fp: String,
    /// Path inside the source repository.
    pub path: String,
    /// The source it belongs to.
    pub source: String,
    /// Manifest file and line, for messages.
    pub origin: (String, usize),
}

impl Doc {
    /// The file name used inside a set: twelve hash digits and the sanitized base name.
    pub fn file_name(&self) -> String {
        let base = self.path.rsplit('/').next().unwrap_or(&self.path);
        let clean: String = base
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let clean = clean.trim_start_matches('.');
        format!("{}-{}", &self.sha256[..12], clean)
    }
}

/// One entry of the reason vocabulary, `reasons.tsv`.
#[derive(Clone, Debug)]
pub struct Reason {
    /// The name, in the vocabulary of the census (`zip.no_eocd`, `xml.depth_ge_100`).
    pub name: String,
    /// What it means, in a sentence.
    pub meaning: String,
    /// Line in `reasons.tsv`.
    pub line: usize,
}

/// One line of `curated.tsv`.
#[derive(Clone, Debug)]
pub struct Curated {
    /// Hash of the document.
    pub sha256: String,
    /// Its source.
    pub source: String,
    /// The id used in the census and in messages.
    pub id: String,
    /// Why it is in the per-pull-request corpus.
    pub reason: String,
    /// Line in `curated.tsv`.
    pub line: usize,
}

/// The classes of compare pairs (`docs/design/compare.md`, 1.2): Harvey LAB turns, the
/// OpenXmlPowerTools comparer pairs, the Docxodus pairs and the document-change-benchmark pairs.
pub const PAIR_CLASSES: &[&str] = &["lab", "oxpt", "dox", "dcb"];

/// One line of `pairs.tsv`.
#[derive(Clone, Debug)]
pub struct Pair {
    /// One of [`PAIR_CLASSES`].
    pub class: String,
    /// Whether the pair runs on every pull request (`curated`) or only in the full run (`full`).
    pub curated: bool,
    /// Hash of the original document.
    pub original: String,
    /// Hash of the revised document.
    pub revised: String,
    /// A name for messages.
    pub label: String,
    /// Why the pair is a noise outlier of N9, or [`NONE`].
    pub outlier: String,
    /// Line in `pairs.tsv`.
    pub line: usize,
}

/// Everything under `corpus/`.
#[derive(Clone, Debug, Default)]
pub struct Manifest {
    /// The sources in file order.
    pub sources: Vec<Source>,
    /// Every document, in manifest file order.
    pub docs: Vec<Doc>,
    /// The per-pull-request selection.
    pub curated: Vec<Curated>,
    /// The reason vocabulary.
    pub reasons: Vec<Reason>,
    /// The compare pairs.
    pub pairs: Vec<Pair>,
    /// Hash of every manifest file, so a cache can tell when it is stale.
    pub hash: String,
}

fn data_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|(i, l)| (i + 1, l))
}

/// Parses `sources.tsv`.
pub fn parse_sources(text: &str) -> Res<Vec<Source>> {
    let mut out = Vec::new();
    for (line, l) in data_lines(text) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() != 6 {
            return Err(format!(
                "corpus/sources.tsv:{line}: expected 6 tab-separated columns, found {}",
                c.len()
            ));
        }
        let how = match c[4] {
            "fetch" => How::Fetch,
            "generate" => How::Generate,
            "host" => How::Host,
            other => {
                return Err(format!(
                    "corpus/sources.tsv:{line}: `how` must be fetch, generate or host, found `{other}`"
                ));
            }
        };
        out.push(Source {
            name: c[0].into(),
            url: c[1].into(),
            commit: c[2].into(),
            spdx: c[3].into(),
            how,
            line,
        });
    }
    Ok(out)
}

/// Parses one `manifest/<source>.tsv`.
pub fn parse_docs(source: &str, file: &str, text: &str) -> Res<Vec<Doc>> {
    let mut out = Vec::new();
    for (line, l) in data_lines(text) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() != 11 {
            return Err(format!(
                "{file}:{line}: expected 11 tab-separated columns, found {}",
                c.len()
            ));
        }
        let size = c[1]
            .parse::<u64>()
            .map_err(|_| format!("{file}:{line}: size `{}` is not a number", c[1]))?;
        out.push(Doc {
            sha256: c[0].into(),
            size,
            tier: c[2].into(),
            expect: c[3].into(),
            stage: c[4].into(),
            part: c[5].into(),
            reason: c[6].into(),
            spdx: c[7].into(),
            producer: c[8].into(),
            word_fp: c[9].into(),
            path: c[10].into(),
            source: source.into(),
            origin: (file.into(), line),
        });
    }
    Ok(out)
}

/// Parses `reasons.tsv`.
pub fn parse_reasons(text: &str) -> Res<Vec<Reason>> {
    let mut out = Vec::new();
    for (line, l) in data_lines(text) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() != 2 {
            return Err(format!(
                "corpus/reasons.tsv:{line}: expected 2 tab-separated columns, found {}",
                c.len()
            ));
        }
        out.push(Reason {
            name: c[0].into(),
            meaning: c[1].into(),
            line,
        });
    }
    Ok(out)
}

/// Parses `curated.tsv`.
pub fn parse_curated(text: &str) -> Res<Vec<Curated>> {
    let mut out = Vec::new();
    for (line, l) in data_lines(text) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() != 4 {
            return Err(format!(
                "corpus/curated.tsv:{line}: expected 4 tab-separated columns, found {}",
                c.len()
            ));
        }
        out.push(Curated {
            sha256: c[0].into(),
            source: c[1].into(),
            id: c[2].into(),
            reason: c[3].into(),
            line,
        });
    }
    Ok(out)
}

/// Parses `pairs.tsv`.
pub fn parse_pairs(text: &str) -> Res<Vec<Pair>> {
    let mut out = Vec::new();
    for (line, l) in data_lines(text) {
        let c: Vec<&str> = l.split('\t').collect();
        if c.len() != 6 {
            return Err(format!(
                "corpus/pairs.tsv:{line}: expected 6 tab-separated columns, found {}",
                c.len()
            ));
        }
        let curated = match c[1] {
            "curated" => true,
            "full" => false,
            other => {
                return Err(format!(
                    "corpus/pairs.tsv:{line}: `set` must be curated or full, found `{other}`"
                ));
            }
        };
        out.push(Pair {
            class: c[0].into(),
            curated,
            original: c[2].into(),
            revised: c[3].into(),
            label: c[4].into(),
            outlier: c[5].into(),
            line,
        });
    }
    Ok(out)
}

/// Lower-case hex of a SHA-256.
pub fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// The text of every manifest file, as read from disk.
pub(super) struct Texts {
    pub(super) sources: String,
    /// `(file, source name, text)` for each `manifest/<source>.tsv`, sorted by file.
    pub(super) docs: Vec<(String, String, String)>,
    pub(super) curated: String,
    pub(super) reasons: String,
    pub(super) pairs: String,
}

impl Texts {
    pub(super) fn read(repo: &Repo) -> Res<Texts> {
        let mut files: Vec<String> = repo
            .files()?
            .into_iter()
            .filter(|f| f.starts_with(&format!("{DIR}/manifest/")) && f.ends_with(".tsv"))
            .collect();
        files.sort();
        let mut docs = Vec::new();
        for file in files {
            let name = file
                .trim_start_matches(&format!("{DIR}/manifest/"))
                .trim_end_matches(".tsv")
                .to_string();
            let text = repo.read(&file)?;
            docs.push((file, name, text));
        }
        let optional = |rel: &str| {
            if repo.exists(rel) {
                repo.read(rel)
            } else {
                Ok(String::new())
            }
        };
        Ok(Texts {
            sources: repo.read(&format!("{DIR}/sources.tsv"))?,
            docs,
            curated: optional(&format!("{DIR}/curated.tsv"))?,
            reasons: optional(&format!("{DIR}/reasons.tsv"))?,
            pairs: optional(&format!("{DIR}/pairs.tsv"))?,
        })
    }

    /// The hash of the files that decide what a set holds. The reason vocabulary only
    /// explains the `reason` entries, so it is left out and an edited meaning does not
    /// rebuild the sets. Of `pairs.tsv` only the class, the set and the two hashes count, so
    /// a recorded outlier or a label can change without rebuilding them.
    pub(super) fn hash(&self) -> String {
        let mut hashed = format!("sources.tsv\n{}\n", self.sources);
        for (file, _, text) in &self.docs {
            hashed.push_str(&format!("{file}\n{text}\n"));
        }
        hashed.push_str(&format!("curated.tsv\n{}\n", self.curated));
        let pairs: Vec<String> = data_lines(&self.pairs)
            .map(|(_, l)| l.split('\t').take(4).collect::<Vec<_>>().join("\t"))
            .collect();
        if !pairs.is_empty() {
            hashed.push_str(&format!("pairs.tsv\n{}\n", pairs.join("\n")));
        }
        hex_sha256(hashed.as_bytes())
    }
}

impl Manifest {
    /// Reads and parses `corpus/` under `repo`.
    pub fn read(repo: &Repo) -> Res<Manifest> {
        Manifest::parse(&Texts::read(repo)?)
    }

    /// Parses the text of the manifest files.
    pub(super) fn parse(texts: &Texts) -> Res<Manifest> {
        let mut docs = Vec::new();
        for (file, name, text) in &texts.docs {
            docs.extend(parse_docs(name, file, text)?);
        }
        Ok(Manifest {
            sources: parse_sources(&texts.sources)?,
            docs,
            curated: parse_curated(&texts.curated)?,
            reasons: parse_reasons(&texts.reasons)?,
            pairs: parse_pairs(&texts.pairs)?,
            hash: texts.hash(),
        })
    }

    /// The manifest hash alone, without parsing the rows. It names the sets in the cache.
    pub fn hash_of(repo: &Repo) -> Res<String> {
        Ok(Texts::read(repo)?.hash())
    }

    /// The source called `name`.
    pub fn source(&self, name: &str) -> Option<&Source> {
        self.sources.iter().find(|s| s.name == name)
    }

    /// The documents of the per-pull-request corpus, with their source's `how`.
    pub fn curated_docs(&self) -> Vec<&Doc> {
        self.docs_by_hash(self.curated.iter().map(|c| c.sha256.as_str()))
    }

    /// The documents of the curated compare pairs, each once: the `curatedpairs` set, fetched with
    /// the curated corpus.
    pub fn curated_pair_docs(&self) -> Vec<&Doc> {
        self.docs_by_hash(
            self.pairs
                .iter()
                .filter(|p| p.curated)
                .flat_map(|p| [p.original.as_str(), p.revised.as_str()]),
        )
    }

    /// The documents of every compare pair, each once: the `pairs` set.
    pub fn pair_docs(&self) -> Vec<&Doc> {
        self.docs_by_hash(
            self.pairs
                .iter()
                .flat_map(|p| [p.original.as_str(), p.revised.as_str()]),
        )
    }

    /// The documents with these hashes, in the order given, each once; unknown hashes are
    /// skipped (the check reports them).
    fn docs_by_hash<'a>(&self, hashes: impl Iterator<Item = &'a str>) -> Vec<&Doc> {
        let by_sha: BTreeMap<&str, &Doc> =
            self.docs.iter().map(|d| (d.sha256.as_str(), d)).collect();
        let mut seen = std::collections::BTreeSet::new();
        hashes
            .filter(|h| seen.insert(*h))
            .filter_map(|h| by_sha.get(h).copied())
            .collect()
    }
}

#[cfg(test)]
pub(super) mod tests;
