//! The complement of a working copy: its entries, the manifest and its canonical form, and `pending_files`.
//!
//! The complement is a few named byte entries under `.stet/`. Everything that a later export may replace is named by the
//! generation (the state version) and by a short digest of its bytes, so that no entry of the manifest that is on disk is
//! ever overwritten, not even by a refresh export at the same generation whose bytes differ. The `manifest` is the text that
//! says which entries make up the working copy, and the one commit point of a write (`write_plan.rs`).
//!
//! **The manifest is read strictly.** It is parsed, written again in its canonical form, and the bytes must be equal, and its
//! last line is the SHA-256 of every byte above it. So any single-byte change to it is refused (law H7): a newer version as
//! `unsupported_version`, anything else as `corrupt_complement`. A change to a file hash that would make an untouched file look
//! hand-edited is refused the same way.

use crate::api::error::{ErrorCode, StetError};
use crate::api::files::ReadableFiles;
use crate::api::view::FormatId;
use crate::api::working_copy::{Complement, WorkingCopy};
use crate::refs::{BaseToken, hex, sha256, unhex};
use std::collections::BTreeMap;

/// The directory of the complement, relative to the working copy.
pub(crate) const DIR: &str = ".stet";
/// The manifest: the one commit point of a write.
pub(crate) const MANIFEST: &str = "manifest";
/// The package that the namespace was derived from, written once.
pub(crate) const ORIGINAL: &str = "original.docx";
/// The interruption marker, present only while a write is in flight.
pub(crate) const MARKER: &str = "writing";
/// The version of the complement this release writes, and the newest it reads.
pub(crate) const VERSION: (u32, u32) = (1, 0);
/// The most bytes a manifest may have: a few hundred bytes and a line for each readable file.
pub(crate) const MAX_MANIFEST_BYTES: usize = 64 << 10;

fn corrupt(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::CorruptComplement, message)
}

/// The path of a complement entry, relative to the working copy's directory.
pub(crate) fn path_of(name: &str) -> String {
    format!("{DIR}/{name}")
}

/// An entry that the manifest names, with the SHA-256 of its bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Named {
    pub(crate) name: String,
    pub(crate) sha256: [u8; 32],
}

/// What the manifest says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Manifest {
    /// The state version the working copy was exported at.
    pub(crate) generation: u32,
    /// The id of the format that wrote the readable files.
    pub(crate) format: String,
    /// The base token of the exported state.
    pub(crate) base: BaseToken,
    /// The SHA-256 of `original.docx`.
    pub(crate) original: [u8; 32],
    /// The head package, or `None` when it equals the original.
    pub(crate) head: Option<Named>,
    /// The ref map.
    pub(crate) refs: Named,
    /// Each readable file as written: its name and the SHA-256 of its text, by name.
    pub(crate) files: Vec<(String, [u8; 32])>,
}

/// The name of an entry that carries a generation: `head.7.1a2b3c4d.docx`, `refs.7.1a2b3c4d.map`.
fn entry_name(stem: &str, generation: u32, sha: &[u8; 32], extension: &str) -> String {
    let digest = hex(sha);
    let short = digest.get(..8).unwrap_or(&digest);
    format!("{stem}.{generation}.{short}.{extension}")
}

/// Whether a name is that of a head or ref-map entry, whichever generation: what recovery may remove.
pub(crate) fn is_generated(name: &str) -> bool {
    let mut parts = name.split('.');
    let stem = parts.next();
    let generation = parts.next();
    let short = parts.next();
    let extension = parts.next();
    let more = parts.next();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let short_ok = short.is_some_and(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    more.is_none()
        && generation.is_some_and(digits)
        && short_ok
        && matches!(
            (stem, extension),
            (Some("head"), Some("docx")) | (Some("refs"), Some("map"))
        )
}

/// Whether a readable file may be named in a manifest: one line, no space, no path.
fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '/' | '\\'))
}

impl Manifest {
    /// The manifest of an export. `files` are the readable files as written, `(name, text)`, in any order.
    pub(crate) fn new(
        base: BaseToken,
        format: &FormatId,
        original: [u8; 32],
        head: [u8; 32],
        refs: [u8; 32],
        files: &[(String, String)],
    ) -> Result<Manifest, StetError> {
        let generation = base.version();
        let mut hashed: Vec<(String, [u8; 32])> = Vec::with_capacity(files.len());
        for (name, text) in files {
            if !valid_file_name(name) {
                return Err(StetError::new(
                    ErrorCode::Internal,
                    format!("internal: {name:?} cannot be the name of a readable file."),
                ));
            }
            hashed.push((name.clone(), sha256(text.as_bytes())));
        }
        hashed.sort();
        if hashed
            .windows(2)
            .any(|w| w.first().map(|f| &f.0) == w.get(1).map(|f| &f.0))
        {
            return Err(StetError::new(
                ErrorCode::Internal,
                "internal: two readable files have one name.",
            ));
        }
        let head = if head == original {
            None
        } else {
            Some(Named {
                name: entry_name("head", generation, &head, "docx"),
                sha256: head,
            })
        };
        Ok(Manifest {
            generation,
            format: format.as_str().to_string(),
            base,
            original,
            head,
            refs: Named {
                name: entry_name("refs", generation, &refs, "map"),
                sha256: refs,
            },
            files: hashed,
        })
    }

    /// The SHA-256 the manifest records for the entry of this name, or `None` if it names no such entry.
    pub(crate) fn digest_of(&self, name: &str) -> Option<[u8; 32]> {
        if name == ORIGINAL {
            return Some(self.original);
        }
        if let Some(head) = self.head.as_ref().filter(|h| h.name == name) {
            return Some(head.sha256);
        }
        (self.refs.name == name).then_some(self.refs.sha256)
    }

    /// The names of the entries the manifest relies on, besides itself.
    pub(crate) fn entry_names(&self) -> Vec<&str> {
        let mut names = vec![ORIGINAL];
        if let Some(head) = &self.head {
            names.push(head.name.as_str());
        }
        names.push(self.refs.name.as_str());
        names
    }

    /// The canonical text, ending in its own digest.
    pub(crate) fn render(&self) -> String {
        let mut out = format!(
            "stetdoc-complement {}.{}\ngeneration {}\nformat {}\nbase {}\noriginal {ORIGINAL} {}\n",
            VERSION.0,
            VERSION.1,
            self.generation,
            self.format,
            self.base,
            hex(&self.original)
        );
        match &self.head {
            Some(head) => out.push_str(&format!("head {} {}\n", head.name, hex(&head.sha256))),
            None => out.push_str("head original\n"),
        }
        out.push_str(&format!(
            "refs {} {}\n",
            self.refs.name,
            hex(&self.refs.sha256)
        ));
        for (name, sha) in &self.files {
            out.push_str(&format!("file {name} {}\n", hex(sha)));
        }
        let digest = hex(&sha256(out.as_bytes()));
        out.push_str(&format!("manifest-sha256 {digest}\n"));
        out
    }

    /// Reads manifest bytes, strictly. See the module docs.
    pub(crate) fn parse(bytes: &[u8]) -> Result<Manifest, StetError> {
        if bytes.len() > MAX_MANIFEST_BYTES {
            return Err(StetError::new(
                ErrorCode::LimitExceeded,
                format!(
                    "The manifest is {} bytes, over the limit of {MAX_MANIFEST_BYTES}.",
                    bytes.len()
                ),
            ));
        }
        let text =
            std::str::from_utf8(bytes).map_err(|_| corrupt("The manifest is not UTF-8 text."))?;
        let version = text.lines().next().unwrap_or_default();
        check_version(version)?;
        let manifest = read(text)?;
        if manifest.render().as_bytes() != bytes {
            return Err(corrupt(
                "The manifest is not in its canonical form, or its own digest does not match. It was edited, or damaged.",
            ));
        }
        Ok(manifest)
    }
}

/// The first line: this release reads its own version and older ones, and a newer major or minor is `unsupported_version`.
fn check_version(line: &str) -> Result<(), StetError> {
    let Some(version) = line.strip_prefix("stetdoc-complement ") else {
        return Err(corrupt(
            "The manifest does not start with `stetdoc-complement`: it is not a manifest.",
        ));
    };
    let parsed = version
        .split_once('.')
        .and_then(|(major, minor)| Some((major.parse::<u32>().ok()?, minor.parse::<u32>().ok()?)));
    match parsed {
        Some((major, minor)) if (major, minor) > VERSION => Err(StetError::new(
            ErrorCode::UnsupportedVersion,
            format!(
                "This working copy was written as complement version {major}.{minor}, and this release reads up to {}.{}.",
                VERSION.0, VERSION.1
            ),
        )),
        Some(_) => Ok(()),
        None => Err(corrupt(
            "The version line of the manifest is not `major.minor`.",
        )),
    }
}

fn digest_of(word: Option<&str>) -> Result<[u8; 32], StetError> {
    word.and_then(unhex::<32>)
        .ok_or_else(|| corrupt("A digest in the manifest is not 64 lower-case hexadecimal digits."))
}

fn read(text: &str) -> Result<Manifest, StetError> {
    let mut lines = text.lines().skip(1);
    let mut field = |key: &str| -> Result<Vec<&str>, StetError> {
        let line = lines
            .next()
            .ok_or_else(|| corrupt(format!("The manifest has no `{key}` line.")))?;
        let mut words = line.split(' ');
        if words.next() != Some(key) {
            return Err(corrupt(format!(
                "The manifest has `{line}` where its `{key}` line belongs."
            )));
        }
        Ok(words.collect())
    };
    let generation = field("generation")?;
    let generation = match generation.as_slice() {
        [n] if *n == "0" || !n.starts_with('0') => n.parse::<u32>().ok(),
        _ => None,
    }
    .ok_or_else(|| corrupt("The generation in the manifest is not a number."))?;
    let format = match field("format")?.as_slice() {
        [id] if !id.is_empty() => (*id).to_string(),
        _ => return Err(corrupt("The format line of the manifest names no format.")),
    };
    let base = match field("base")?.as_slice() {
        [token] => BaseToken::parse(token)
            .map_err(|_| corrupt("The base token in the manifest is not a base token."))?,
        _ => return Err(corrupt("The base line of the manifest is not one token.")),
    };
    let original = match field("original")?.as_slice() {
        [name, sha] if *name == ORIGINAL => digest_of(Some(sha))?,
        _ => {
            return Err(corrupt(
                "The original line of the manifest does not name `original.docx` and its digest.",
            ));
        }
    };
    let head_words = field("head")?;
    let head = match head_words.as_slice() {
        ["original"] => None,
        [name, sha] => Some(Named {
            name: (*name).to_string(),
            sha256: digest_of(Some(sha))?,
        }),
        _ => {
            return Err(corrupt(
                "The head line of the manifest is not `original` or a name and a digest.",
            ));
        }
    };
    let refs = match field("refs")?.as_slice() {
        [name, sha] => Named {
            name: (*name).to_string(),
            sha256: digest_of(Some(sha))?,
        },
        _ => {
            return Err(corrupt(
                "The refs line of the manifest is not a name and a digest.",
            ));
        }
    };
    let mut files = Vec::new();
    let mut rest = lines.peekable();
    while rest.peek().is_some_and(|l| l.starts_with("file ")) {
        let line = rest.next().unwrap_or_default();
        let mut words = line.split(' ').skip(1);
        let name = words.next().unwrap_or_default();
        if !valid_file_name(name) {
            return Err(corrupt(
                "A file line of the manifest has a name that no readable file has.",
            ));
        }
        let sha = digest_of(words.next())?;
        if words.next().is_some() {
            return Err(corrupt(
                "A file line of the manifest has more than a name and a digest.",
            ));
        }
        files.push((name.to_string(), sha));
    }
    // The last line is the manifest's own digest. It is checked by rendering again and comparing every byte.
    match (rest.next(), rest.next()) {
        (Some(last), None) if last.starts_with("manifest-sha256 ") => {}
        _ => return Err(corrupt("The manifest does not end with its own digest.")),
    }
    if base.version() != generation {
        return Err(corrupt(
            "The generation of the manifest is not the version of its base token.",
        ));
    }
    let named_right = head
        .as_ref()
        .is_none_or(|h| h.name == entry_name("head", generation, &h.sha256, "docx"))
        && refs.name == entry_name("refs", generation, &refs.sha256, "map");
    if !named_right {
        return Err(corrupt(
            "An entry of the manifest is not named by its generation and its digest.",
        ));
    }
    Ok(Manifest {
        generation,
        format,
        base,
        original,
        head,
        refs,
        files,
    })
}

impl Complement {
    /// The bytes of the entry with this name. Two entries of one name are damage.
    pub(crate) fn entry(&self, name: &str) -> Result<Option<&[u8]>, StetError> {
        let mut found = self.entries.iter().filter(|(n, _)| n == name);
        let first = found.next();
        if found.next().is_some() {
            return Err(corrupt(format!(
                "The complement has two entries named `{name}`."
            )));
        }
        Ok(first.map(|(_, bytes)| bytes.as_slice()))
    }
}

/// The refusal of a working copy whose write was interrupted and not recovered.
fn interrupted() -> StetError {
    corrupt(
        "A write to this working copy was interrupted: `.stet/writing` exists. Recover it first, with `WorkingCopy::recover`, then retry.",
    )
    .with_next(
        "Recover the directory before any other command. Recovery finishes or undoes the write, and keeps every hand edit.",
    )
}

impl WorkingCopy {
    /// The manifest of this working copy, strictly read. Absent is `corrupt_complement`, and so is a working copy that still
    /// has the interruption marker (the entry `writing`): the caller read the directory as it was in the middle of a write,
    /// and its files may be the old render under the new manifest, which would read as hand edits. `recover` is the one reader
    /// that accepts the marker.
    pub(crate) fn manifest(&self) -> Result<Manifest, StetError> {
        if self.complement.entry(MARKER)?.is_some() {
            return Err(interrupted());
        }
        self.read_manifest()
    }

    /// The manifest, with no regard for the marker.
    pub(crate) fn read_manifest(&self) -> Result<Manifest, StetError> {
        match self.complement.entry(MANIFEST)? {
            Some(bytes) => Manifest::parse(bytes),
            None => Err(corrupt(
                "The complement has no manifest. If this directory is an interrupted unpack, delete it and unpack the document again.",
            )),
        }
    }

    /// The readable files that differ from what was written, as files for `Edit::files`, or `None` when none do.
    ///
    /// It compares the SHA-256 of each file the manifest names with the text in `files`. A file that was written and is now
    /// missing is `invalid_payload`. **A file that was never written is `unsupported`**, because dropping text an agent wrote
    /// is the one thing a working copy must not do: a `notes.md` for a document with no notes is the agent trying to add a
    /// footnote from text.
    pub fn pending_files(&self) -> Result<Option<ReadableFiles>, StetError> {
        let manifest = self.manifest()?;
        let mut here: BTreeMap<&str, &str> = BTreeMap::new();
        for (name, text) in &self.files {
            if here.insert(name.as_str(), text.as_str()).is_some() {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    format!("The working copy has two files named `{name}`."),
                ));
            }
        }
        let written: BTreeMap<&str, &[u8; 32]> = manifest
            .files
            .iter()
            .map(|(n, s)| (n.as_str(), s))
            .collect();
        for name in here.keys() {
            if !written.contains_key(name) {
                return Err(never_written(name));
            }
        }
        let mut pending = Vec::new();
        let mut rendered = Vec::new();
        for (name, sha) in &manifest.files {
            let Some(text) = here.get(name.as_str()) else {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    format!(
                        "`{name}` was written and is now missing. Restore it, or export the document again."
                    ),
                ));
            };
            if sha256(text.as_bytes()) != *sha {
                pending.push((name.clone(), (*text).to_string()));
                rendered.push((name.clone(), hex(sha)));
            }
        }
        if pending.is_empty() {
            return Ok(None);
        }
        Ok(Some(ReadableFiles {
            base: manifest.base.to_string(),
            format: FormatId::new(manifest.format),
            files: pending,
            rendered,
        }))
    }

    /// `pending_edits` when readable files hold hand edits that were not packed. The CLI calls it before `edit <dir>`.
    pub fn ensure_clean(&self) -> Result<(), StetError> {
        match self.pending_files()? {
            None => Ok(()),
            Some(files) => {
                let names: Vec<String> =
                    files.files.iter().map(|(n, _)| format!("`{n}`")).collect();
                Err(StetError::new(
                    ErrorCode::PendingEdits,
                    format!(
                        "{} {} hand edits that have not been packed, so an operation would lose or mix them.",
                        names.join(", "),
                        if names.len() == 1 { "has" } else { "have" }
                    ),
                )
                .with_next("Pack the hand edits (`stetdoc pack`), or restore the files, then retry the operation."))
            }
        }
    }
}

/// What to say about a file the manifest never named: which operation does what the agent was trying to do.
fn never_written(name: &str) -> StetError {
    let why = match name {
        "notes.md" => "the document has no footnotes or endnotes, so text here cannot add one",
        "headers.md" => "the document has no headers or footers, so text here cannot add one",
        "comments.md" => {
            "comments are read-only here: use the `comment`, `reply` and `resolve` operations"
        }
        _ => "it is not a file of this working copy, and text in it would be dropped",
    };
    StetError::new(
        ErrorCode::Unsupported,
        format!("`{name}` was never written for this document: {why}."),
    )
    .with_next("Delete the file, and make the change with an operation or in a file this working copy has.")
}

#[cfg(test)]
mod tests;
