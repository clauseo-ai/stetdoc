//! The working copy as a value, how it is written to disk, and what `save` returns.
//!
//! A working copy is the readable files and the complement. The core does no I/O: the CLI writes the files into the
//! directory and the complement into `.stet/`, and napi and the editor keep both in memory. How the CLI writes them is
//! the core's: `WorkingCopy::write_plan` returns the steps, in the order that makes a crash harmless, and
//! `WorkingCopy::recover` finishes or undoes an interrupted write.

use super::revision::RevisionInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The entries of `.stet/`, by name, with their bytes. What they are, and how they are named, is in `document/complement.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Complement {
    /// `(name, bytes)` for each entry, names relative to `.stet/`.
    pub entries: Vec<(String, Vec<u8>)>,
}

/// The readable files and the complement: what `export` returns and `resume` reads. The agent edits the files and never the
/// complement.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkingCopy {
    /// `(name, text)` for each readable file: the top-level files of the directory whose extension the format uses.
    pub files: Vec<(String, String)>,
    /// What `.stet/` holds.
    pub complement: Complement,
}

/// What a write of a working copy may do beyond the safe default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WriteOptions {
    /// The readable files whose hand edits this export took in, which the plan may overwrite.
    pub consumed: Vec<String>,
    /// Write every readable file again, overwriting hand edits. Only when the agent asked.
    pub refresh: bool,
}

/// One step of writing a working copy. The CLI executes them in order. Every write is a temporary file, an fsync and an
/// atomic rename, so a file is whole or absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteStep {
    /// The interruption marker, `.stet/writing`, written first.
    Marker {
        /// The path, relative to the working copy's directory.
        path: String,
        /// The marker's text.
        bytes: Vec<u8>,
    },
    /// A new complement entry. Its name is new, so the manifest on disk is still valid.
    Entry {
        /// The path, relative to the working copy's directory.
        path: String,
        /// The entry's bytes.
        bytes: Vec<u8>,
    },
    /// The manifest: the one commit point.
    Commit {
        /// The path, relative to the working copy's directory.
        path: String,
        /// The manifest's bytes.
        bytes: Vec<u8>,
    },
    /// A readable file, written after the commit.
    File {
        /// The path, relative to the working copy's directory.
        path: String,
        /// The file's text.
        text: String,
    },
    /// A stale entry, or the marker at the very end. A file that is already gone is not an error.
    Remove {
        /// The path, relative to the working copy's directory.
        path: String,
    },
}

/// The steps that write a working copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WritePlan {
    /// The steps, in the order to execute them.
    pub steps: Vec<WriteStep>,
}

/// What to do about a directory that carries an interruption marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recovery {
    /// The writes and removals that finish or undo the interrupted write, including the copies that save an edit aside.
    /// Executing them again is harmless.
    pub steps: Vec<WriteStep>,
    /// The removal of the marker. Run it last, after `steps` and after writing the files of `rerender`, so that a crash
    /// before it is recovered again from the same marker.
    pub finish: WriteStep,
    /// The readable files to render again from the committed head and write, after `steps` and before `finish`. To render
    /// them, resume the directory without its `writing` entry, which `Document::resume` refuses.
    pub rerender: Vec<String>,
    /// The readable files with hand edits made after the crash that stay pending: the edit is against the render the new
    /// manifest records, so it is exactly what `pending_files` will report.
    pub kept: Vec<String>,
    /// What the caller tells the agent before it runs the steps: an edit made after the crash on a render that the commit
    /// replaced, which was saved aside under the name the warning gives, and which the agent has to redo.
    pub warnings: Vec<String>,
}

/// What `Document::save` returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Saved {
    /// The package. An unedited document returns the bytes it was opened with, exactly.
    pub bytes: Vec<u8>,
    /// What the saved package holds.
    pub receipt: SaveReceipt,
}

/// A summary of a saved package.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SaveReceipt {
    /// The base token of the saved state.
    pub base: String,
    /// False when the bytes are the input's, exactly.
    pub edited: bool,
    /// The SHA-256 of the bytes, in hex.
    pub sha256: String,
    /// Every revision of the saved package.
    pub revisions: Vec<RevisionInfo>,
}
