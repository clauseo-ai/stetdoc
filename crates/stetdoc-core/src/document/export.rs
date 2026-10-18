//! `export` and `resume`: a document into a working copy and back.
//!
//! `export` hashes the readable files and the original, asks `State::export` for the head, the ref map and the digests, names
//! the entries by generation and digest, and ends the manifest with its own digest. Entries and manifest lines are in a fixed
//! order, so the same state gives the same bytes (law L4). `resume` is the strict inverse: it trusts nothing in the working copy
//! but the manifest, which it reads canonically, and everything else is checked against the digests the manifest holds.

use super::complement::{MANIFEST, Manifest, ORIGINAL};
use super::{Document, Original, check_format};
use crate::api::error::{ErrorCode, StetError};
use crate::api::options::OpenOptions;
use crate::api::view::FormatId;
use crate::api::working_copy::{Complement, WorkingCopy};
use crate::refs::sha256;
use crate::state::{Recorded, State};
use crate::xml::Bytes;

fn corrupt(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::CorruptComplement, message)
}

fn over(what: &str, size: usize, limit: u64) -> StetError {
    StetError::new(
        ErrorCode::LimitExceeded,
        format!("The complement entry `{what}` is {size} bytes, over the limit of {limit}."),
    )
    .with_next(
        "Use a smaller document, or raise the limit in the open options if the source is trusted.",
    )
}

impl Document {
    /// The readable files and the complement. The same state gives the same bytes.
    pub fn export(&self) -> Result<WorkingCopy, StetError> {
        let files = self.rendered_files()?;
        self.export_with(files)
    }

    /// The readable files of the current state, `(name, text)`, as the projection renders them in the document's format.
    fn rendered_files(&self) -> Result<Vec<(String, String)>, StetError> {
        Ok(self.files()?.files)
    }

    /// An export with these readable files: the seam that the projection's renderer fills, and that tests of the write protocol
    /// use to give a working copy files before the renderer exists.
    pub(crate) fn export_with(
        &self,
        mut files: Vec<(String, String)>,
    ) -> Result<WorkingCopy, StetError> {
        self.check_original()?;
        let exported = self.state.export()?;
        // An unedited document's original is what it writes, and a kept one is what it was opened from.
        let original = match &self.original {
            Original::Opened => None,
            Original::Kept(bytes) => Some(bytes.clone()),
        };
        let head_is_original = original
            .as_ref()
            .is_none_or(|o| o.as_slice() == exported.head.as_slice());
        let original_sha = original
            .as_ref()
            .map_or(exported.head_sha256, |o| sha256(o));
        let manifest = Manifest::new(
            exported.base,
            &self.format,
            original_sha,
            exported.head_sha256,
            exported.refs_sha256,
            &files,
        )?;
        files.sort();
        let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(4);
        match original {
            None => entries.push((ORIGINAL.to_string(), exported.head)),
            Some(original) => {
                entries.push((ORIGINAL.to_string(), original.to_vec()));
                if let (false, Some(head)) = (head_is_original, &manifest.head) {
                    entries.push((head.name.clone(), exported.head));
                }
            }
        }
        entries.push((manifest.refs.name.clone(), exported.refs));
        entries.push((MANIFEST.to_string(), manifest.render().into_bytes()));
        Ok(WorkingCopy {
            files,
            complement: Complement { entries },
        })
    }

    /// Opens the document that `export` saved, with its refs. The readable files of the working copy are not read: the agent
    /// can write to the directory, so `resume` trusts nothing in it, and the files matter only to `pending_files`.
    ///
    /// Counters, stamps and lineage continue where they were, so a batch written before the export is still valid after it. The
    /// document takes the format recorded in the manifest, and `options.format` applies to `open` only.
    pub fn resume(
        working_copy: &WorkingCopy,
        options: &OpenOptions,
    ) -> Result<Document, StetError> {
        let limits = &options.limits;
        let manifest = working_copy.manifest()?;
        check_format(&FormatId::new(manifest.format.clone())).map_err(|_| {
            StetError::new(
                ErrorCode::UnsupportedVersion,
                format!(
                    "This release has no adapter for the format `{}` of this working copy.",
                    manifest.format
                ),
            )
        })?;
        let complement = &working_copy.complement;
        let entry = |name: &str, bound: u64| -> Result<&[u8], StetError> {
            let bytes = complement.entry(name)?.ok_or_else(|| {
                corrupt(format!(
                    "The complement has no entry `{name}`, which the manifest names."
                ))
            })?;
            if bytes.len() as u64 > bound {
                return Err(over(name, bytes.len(), bound));
            }
            Ok(bytes)
        };
        let original = entry(ORIGINAL, limits.max_archive_bytes)?;
        if sha256(original) != manifest.original {
            return Err(corrupt(
                "`original.docx` does not match the digest recorded in the manifest.",
            ));
        }
        let head = match &manifest.head {
            Some(named) => entry(&named.name, limits.max_archive_bytes)?.to_vec(),
            None => original.to_vec(),
        };
        let head_sha256 = manifest
            .head
            .as_ref()
            .map_or(manifest.original, |h| h.sha256);
        let refs = entry(&manifest.refs.name, limits.max_entry_bytes)?;
        let recorded = Recorded {
            base: manifest.base,
            head_sha256,
            refs_sha256: manifest.refs.sha256,
        };
        let state = State::resume(head, refs, &recorded, Some(original), limits)?;
        Ok(Document {
            state,
            limits: limits.clone(),
            format: FormatId::new(manifest.format),
            original: Original::Kept(Bytes::from(original.to_vec())),
        })
    }
}
