//! What a command's `<doc>` argument names: a package, or a working copy (a directory).

use crate::fsio;
use crate::fsio::workdir::Workdir;
use crate::output::Failure;
use std::path::{Path, PathBuf};
use stetdoc_core::{Document, Limits, OpenOptions};

/// A package or a working copy.
pub enum Source {
    /// A `.docx` file.
    Docx(PathBuf),
    /// A working copy's directory.
    Dir(PathBuf),
}

impl Source {
    /// A directory is a working copy, anything else a package. A path that is not there is `not_found`, whatever it was
    /// meant to be.
    pub fn of(path: &Path) -> Result<Source, Failure> {
        fsio::present(path)?;
        Ok(if path.is_dir() {
            Source::Dir(path.to_path_buf())
        } else {
            Source::Docx(path.to_path_buf())
        })
    }
}

/// Reads and opens a package.
pub fn open_docx(path: &Path) -> Result<Document, Failure> {
    let options = OpenOptions::default();
    let bytes = fsio::read_file(path, options.limits.max_archive_bytes)?;
    Ok(Document::open(bytes, &options)?)
}

/// Reads a package's bytes, bounded by the default limits.
pub fn read_docx(path: &Path) -> Result<Vec<u8>, Failure> {
    fsio::read_file(path, Limits::default().max_archive_bytes)
}

/// A document opened for a command that only reads, with the notes to print on stderr.
pub struct Read {
    /// The document.
    pub doc: Document,
    /// Notes for stderr: a recovery's warnings, and hand edits that the answer does not show.
    pub notes: Vec<String>,
}

/// Opens a package, or locks, recovers and resumes a working copy. A working copy answers from its last export, so pending
/// hand edits get a note.
pub fn open_read(path: &Path) -> Result<Read, Failure> {
    match Source::of(path)? {
        Source::Docx(path) => Ok(Read {
            doc: open_docx(&path)?,
            notes: Vec::new(),
        }),
        Source::Dir(path) => {
            let (workdir, mut notes) = Workdir::open(&path)?;
            let (doc, copy) = workdir.resume()?;
            if let Ok(Some(pending)) = copy.pending_files() {
                let names: Vec<&str> = pending.files.iter().map(|(n, _)| n.as_str()).collect();
                notes.push(format!(
                    "note: {} has edits that are not packed, and this shows the last export. stetdoc check shows what they would do.",
                    names.join(", ")
                ));
            }
            Ok(Read { doc, notes })
        }
    }
}
