//! Reads the corpus sets that `cargo xtask corpus fetch` builds.
//!
//! The cache root is `STETDOC_CORPUS_DIR`. A set is `<root>/sets/<name>/` with an
//! `INDEX.tsv`, and it is usable only when its `COMPLETE` marker exists. The default set
//! is `curated`; `STETDOC_CORPUS_SET` names another, such as `full`, or one built from an
//! exact manifest, such as `curated-0123456789ab`. A set is never changed after it is built.

use crate::expect::Expect;
use std::path::{Path, PathBuf};

/// The environment variable that names the cache root.
pub const ENV_DIR: &str = "STETDOC_CORPUS_DIR";
/// The environment variable that names the set to read.
pub const ENV_SET: &str = "STETDOC_CORPUS_SET";

/// One document of a set.
#[derive(Clone, Debug)]
pub struct Entry {
    /// `<source>:<path in the source>`.
    pub id: String,
    /// `lab`, `word-authored`, `other-producers`, `public`, `edge` or `hostile`.
    pub tier: String,
    /// What stetdoc must do with it, and where a refusal happens.
    pub expect: Expect,
    /// Lower-case SHA-256 of the file.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// Where the file is.
    pub path: PathBuf,
    /// Producer family read from the document properties.
    pub producer: String,
    /// `y` when the document looks written by real Word, `n` when not, `?` when unscanned.
    pub word_fp: String,
    /// Licence of the document.
    pub spdx: String,
    /// Why the document is hostile or refused, in the vocabulary of `corpus/reasons.tsv`;
    /// `-` for an ordinary document.
    pub reason: String,
    /// The parts that carry a refusal at the `read` or `xml` stage. A layer that never reads
    /// them does not meet the refusal (see [`Entry::seen_by_reading`]). Empty for every other
    /// document. The law that goes with it: every part that is not named reads and parses
    /// cleanly, which `seen_by_reading` relies on and the container laws of the core check.
    pub parts: Vec<String>,
}

/// A set of documents.
#[derive(Clone, Debug)]
pub struct Corpus {
    /// The documents, in index order.
    pub entries: Vec<Entry>,
}

/// Parses an `INDEX.tsv`, resolving paths against `set_dir`.
pub fn parse_index(text: &str, set_dir: &Path) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() != 12 {
            return Err(format!(
                "INDEX.tsv line {}: expected 12 columns, found {}",
                i + 1,
                c.len()
            ));
        }
        entries.push(Entry {
            id: c[0].into(),
            tier: c[1].into(),
            expect: Expect::from_columns(c[2], c[9])
                .map_err(|e| format!("INDEX.tsv line {}: {e}", i + 1))?,
            sha256: c[3].into(),
            size: c[4]
                .parse()
                .map_err(|_| format!("INDEX.tsv line {}: bad size", i + 1))?,
            path: set_dir.join(c[5]),
            producer: c[6].into(),
            word_fp: c[7].into(),
            spdx: c[8].into(),
            reason: c[10].into(),
            parts: if c[11] == "-" {
                Vec::new()
            } else {
                c[11].split(';').map(String::from).collect()
            },
        });
    }
    Ok(entries)
}

impl Corpus {
    /// Reads the set `name` under `root`. Fails when the set is missing or incomplete.
    ///
    /// `sets/<name>` may be a link that a later fetch moves to a newer set. It is resolved
    /// once, here, so the entries keep pointing into the directory that was read.
    pub fn open(root: &Path, name: &str) -> Result<Corpus, String> {
        let link = root.join("sets").join(name);
        let dir = link
            .canonicalize()
            .ok()
            .filter(|dir| dir.join("COMPLETE").exists())
            .ok_or_else(|| {
                format!(
                    "the corpus set {} is missing or incomplete; run `cargo xtask corpus fetch`",
                    link.display()
                )
            })?;
        let text = std::fs::read_to_string(dir.join("INDEX.tsv"))
            .map_err(|e| format!("cannot read INDEX.tsv: {e}"))?;
        Ok(Corpus {
            entries: parse_index(&text, &dir)?,
        })
    }

    /// Reads the set named by the environment, or `None` when `STETDOC_CORPUS_DIR` is unset
    /// (a test then skips, naming the variable). The variable may hold several roots separated
    /// by `:`; the first one that has the set wins.
    pub fn from_env() -> Option<Result<Corpus, String>> {
        let dirs = std::env::var(ENV_DIR).ok().filter(|d| !d.is_empty())?;
        let set = std::env::var(ENV_SET).unwrap_or_else(|_| "curated".into());
        let mut last = Err(format!("{ENV_DIR} names no usable directory"));
        for dir in dirs.split(':').filter(|d| !d.is_empty()) {
            last = Corpus::open(Path::new(dir), &set);
            if last.is_ok() {
                break;
            }
        }
        Some(last)
    }

    /// The documents of one tier.
    pub fn tier<'a>(&'a self, tier: &'a str) -> impl Iterator<Item = &'a Entry> {
        self.entries.iter().filter(move |e| e.tier == tier)
    }

    /// The documents that must open and round-trip: every tier, `expect` ok.
    pub fn expected_ok(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.expect.is_ok())
    }

    /// The documents that must be refused.
    pub fn expected_refused(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| !e.expect.is_ok())
    }
}

impl Entry {
    /// What a layer must observe if it reads only the parts `reads` accepts and reaches the
    /// stages up to `last`: see [`Expect::seen_by_reading`].
    pub fn seen_by_reading(
        &self,
        last: crate::expect::Stage,
        reads: impl Fn(&str) -> bool,
    ) -> Expect {
        let parts: Vec<&str> = self.parts.iter().map(String::as_str).collect();
        self.expect.seen_by_reading(last, &parts, reads)
    }

    /// The file's bytes.
    pub fn bytes(&self) -> Vec<u8> {
        std::fs::read(&self.path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = "id\ttier\texpect\tsha256\tsize\tpath\tproducer\tword_fp\tspdx\tstage\treason\tpart\ns:a.docx\tlab\tok\taaa\t3\tlab/aaa-a.docx\tpython-docx\tn\tMIT\t-\t-\t-\ns:b.docx\thostile\tmalformed_input\tbbb\t4\thostile/bbb-b.docx\tunknown\t?\tMIT\topen\tzip.no_eocd\t-\ns:c.docx\thostile\tlimit_exceeded\tccc\t5\thostile/ccc-c.docx\tunknown\t?\tMIT\tread\tzip.high_compression_ratio\tdocProps/app.xml;word/theme/theme1.xml\n";

    #[test]
    fn an_index_parses_into_entries_with_outcomes() {
        let entries = parse_index(INDEX, Path::new("/set")).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].expect, Expect::Ok);
        assert!(entries[0].parts.is_empty() && entries[1].parts.is_empty());
        assert_eq!(
            entries[2].parts,
            ["docProps/app.xml", "word/theme/theme1.xml"]
        );
        assert_eq!(entries[1].expect, Expect::open("malformed_input"));
        assert_eq!(entries[1].path, Path::new("/set/hostile/bbb-b.docx"));
        assert_eq!(entries[1].reason, "zip.no_eocd");
        assert!(parse_index("h\nonly\tthree\tcolumns\n", Path::new("/")).is_err());
        let bad_code = INDEX.replace("malformed_input\tbbb", "stale_ref\tbbb");
        assert!(
            parse_index(&bad_code, Path::new("/"))
                .unwrap_err()
                .contains("line 3")
        );
        let bad_stage = INDEX.replace("\topen\t", "\t-\t");
        assert!(parse_index(&bad_stage, Path::new("/")).is_err());
    }

    #[test]
    fn a_layer_that_does_not_read_the_part_of_a_refusal_sees_ok() {
        use crate::expect::Stage;
        let entries = parse_index(INDEX, Path::new("/set")).unwrap();
        let ratio = &entries[2];
        assert_eq!(
            ratio.seen_by_reading(Stage::Document, |p| p == "word/document.xml"),
            Expect::Ok
        );
        assert_eq!(
            ratio.seen_by_reading(Stage::Document, |p| p == "word/theme/theme1.xml"),
            Expect::read("limit_exceeded")
        );
        let open = &entries[1];
        assert_eq!(
            open.seen_by_reading(Stage::Open, |_| false),
            Expect::open("malformed_input")
        );
    }

    #[test]
    fn an_incomplete_set_is_refused_with_the_command_to_run() {
        let dir = tempfile::tempdir().unwrap();
        let err = Corpus::open(dir.path(), "curated").unwrap_err();
        assert!(err.contains("cargo xtask corpus fetch"), "{err}");
    }

    #[test]
    fn a_complete_set_opens_and_filters_by_tier_and_outcome() {
        let dir = tempfile::tempdir().unwrap();
        let set = dir.path().join("sets/curated");
        std::fs::create_dir_all(&set).unwrap();
        std::fs::write(set.join("INDEX.tsv"), INDEX).unwrap();
        std::fs::write(set.join("COMPLETE"), "hash\n2\n").unwrap();
        let corpus = Corpus::open(dir.path(), "curated").unwrap();
        assert_eq!(corpus.tier("lab").count(), 1);
        assert_eq!(corpus.expected_ok().count(), 1);
        assert_eq!(corpus.expected_refused().next().unwrap().id, "s:b.docx");
    }

    #[cfg(unix)]
    #[test]
    fn a_set_read_through_a_link_keeps_its_directory_when_the_link_moves() {
        let dir = tempfile::tempdir().unwrap();
        let sets = dir.path().join("sets");
        for (name, body) in [("curated-1", "one"), ("curated-2", "two")] {
            let set = sets.join(name);
            std::fs::create_dir_all(set.join("lab")).unwrap();
            std::fs::write(set.join("INDEX.tsv"), INDEX).unwrap();
            std::fs::write(set.join("COMPLETE"), "hash\n2\n").unwrap();
            std::fs::write(set.join("lab/aaa-a.docx"), body).unwrap();
        }
        std::os::unix::fs::symlink("curated-1", sets.join("curated")).unwrap();
        let corpus = Corpus::open(dir.path(), "curated").unwrap();
        std::fs::remove_file(sets.join("curated")).unwrap();
        std::os::unix::fs::symlink("curated-2", sets.join("curated")).unwrap();
        assert_eq!(corpus.entries[0].bytes(), b"one");
        assert_eq!(
            Corpus::open(dir.path(), "curated").unwrap().entries[0].bytes(),
            b"two"
        );
    }
}
