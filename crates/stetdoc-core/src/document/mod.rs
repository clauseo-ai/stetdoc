//! The public handle and orchestration.
//!
//! **Owns.** `Document`: the orchestration of stage, ops, check, scope guard and commit; the working copy (`export`, `resume`,
//! the manifest, `pending_files`, `write_plan` and `recover`); `save`; and the public `compare` entry.
//!
//! **Interface.** The public interface, as listed in `ARCHITECTURE.md`: the thirteen methods of `Document`, `WorkingCopy::{pending_files,
//! ensure_clean, write_plan, recover}`, and the free functions `compare`, `capabilities` and `json_schema`.
//!
//! **Invariants.**
//! - `edit` is atomic: on an error or a dry run the `Document` is unchanged.
//! - An unedited `Document` saves the input bytes exactly (law L1), and `export`, `resume`, `export` is stable (law L4).
//! - Tracked by default, and a `Stamp` has no default.
//! - The manifest is the one commit point of a write and is read strictly: any single-byte change to it is refused (law H7).
//!   An entry that a later export may replace is named by generation and digest, so a committed entry is never overwritten.
//! - A hand edit to a readable file is never overwritten by a write, and an interrupted write recovers to exactly the old
//!   working copy or exactly the new one (law H12).
//!
//! **Depends on.** any module in the core.
//!
//! **Start reading.** `document/mod.rs`, then `document/complement.rs` for the working copy and `document/write_plan.rs` for how it is
//! written.
//!
//! The design is in `docs/design/document-and-ops.md`.

mod blocks;
mod capabilities;
mod changes;
mod checking;
mod compare;
mod complement;
mod declared;
mod edit;
mod edit_files;
mod export;
mod inspect;
mod inspect_lists;
mod receipt;
mod receipt_text;
mod write_plan;

use crate::api::error::{ErrorCode, StetError};
use crate::api::files::ReadableFiles;
use crate::api::limits::Limits;
use crate::api::lookup::Lookup;
use crate::api::options::OpenOptions;
use crate::api::view::{FormatId, RevisionFilter, View, ViewSpec};
use crate::api::working_copy::{SaveReceipt, Saved};
use crate::projection;
use crate::refs::{hex, sha256};
use crate::state::State;
use crate::xml::Bytes;

pub use capabilities::{capabilities, json_schema};
pub use compare::{Compared, compare};

/// The package the namespace was derived from.
enum Original {
    /// Not kept: the document is unedited, so its original is what `save` writes, which law L1 makes the input bytes. The first
    /// edit (step 3) captures the bytes before it installs a state, and from then on the document holds `Kept`.
    Opened,
    /// The bytes `open` was given, or the `original.docx` of a resumed complement.
    Kept(Bytes),
}

/// An opened package and everything stetdoc derives from it.
///
/// It holds the committed `State`, the limits it was opened under, the format of its working copy and its original. Nothing in
/// it is mutated by a method that takes `&self`, apart from the caches that `State` keeps, so it is `Send` and not `Sync`.
pub struct Document {
    state: State,
    limits: Limits,
    format: FormatId,
    original: Original,
}

/// The only format with readable files is `stet/1`.
fn check_format(format: &FormatId) -> Result<(), StetError> {
    if format.as_str() == FormatId::STET {
        return Ok(());
    }
    Err(StetError::new(
        ErrorCode::Unsupported,
        format!(
            "The format `{}` has no readable files. A working copy is written in `{}`.",
            format.as_str(),
            FormatId::STET
        ),
    ))
}

impl Document {
    /// `Original::Opened` stands for the package a document was opened from only while the document is unedited. `edit`
    /// (step 3) keeps the original before it installs its first state, and a document at a later version that has not kept it
    /// would make `save` and `export` take its edited head for its original: a wrong complement, which `resume` would accept.
    /// So that mistake is an error, and not a silent one.
    fn check_original(&self) -> Result<(), StetError> {
        if matches!(self.original, Original::Opened) && self.state.base_token().version() != 0 {
            return Err(StetError::new(
                ErrorCode::Internal,
                "internal: a document that was edited has not kept the package it was opened from, so it cannot say what its original is.",
            ));
        }
        Ok(())
    }

    /// Opens a package. Every `open` starts a ref namespace, a hash of the bytes, so the same bytes always give the same refs.
    pub fn open(bytes: Vec<u8>, options: &OpenOptions) -> Result<Document, StetError> {
        check_format(&options.format)?;
        let state = State::open(bytes, &options.limits)?;
        Ok(Document {
            state,
            limits: options.limits.clone(),
            format: options.format.clone(),
            original: Original::Opened,
        })
    }

    /// The base token of the current state: what the next batch must name.
    pub fn base(&self) -> String {
        self.state.base_token().to_string()
    }

    /// One page of a view of the current state. The view of a working copy shows the state at its last export and ignores hand
    /// edits that were not packed, because the caller asks the working copy and not the files.
    pub fn view(&self, spec: &ViewSpec) -> Result<View, StetError> {
        projection::view(&self.state, spec)
    }

    /// Serializes the document. An unedited document returns the bytes it was opened with, exactly (law L1). It validates
    /// nothing, because every edit was validated before it committed.
    pub fn save(&self) -> Result<Saved, StetError> {
        self.check_original()?;
        let bytes = self.state.to_package()?.write()?;
        let edited = match &self.original {
            Original::Opened => false,
            Original::Kept(original) => original.as_slice() != bytes.as_slice(),
        };
        let receipt = SaveReceipt {
            base: self.base(),
            edited,
            sha256: hex(&sha256(&bytes)),
            revisions: self.revisions(&RevisionFilter::default())?,
        };
        Ok(Saved { bytes, receipt })
    }

    /// Defined terms, cross-references, and what refers to a block. `Lookup.text` is the answer as the document's format writes
    /// it, which is what a surface prints.
    pub fn lookup(&self, query: &str) -> Result<Lookup, StetError> {
        let mut found = projection::lookup(&self.state, query)?;
        found.text = projection::render_lookup(&self.state, &found, &self.format)?;
        Ok(found)
    }

    /// The readable files, without the complement. They carry the base token and the format, and no `rendered` hashes: a caller
    /// that edits them in the same process cannot have changed release.
    pub fn files(&self) -> Result<ReadableFiles, StetError> {
        projection::render_files(&self.state, &self.format)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_check;
#[cfg(test)]
mod tests_compare;
#[cfg(test)]
mod tests_edit;
#[cfg(test)]
mod tests_edit_comments;
#[cfg(test)]
mod tests_edit_files;
#[cfg(test)]
mod tests_edit_foreign;
#[cfg(test)]
mod tests_edit_ops;
#[cfg(test)]
mod tests_edit_props;
#[cfg(test)]
mod tests_edit_reject;
#[cfg(test)]
mod tests_edit_review;
#[cfg(test)]
mod tests_edit_toc;
#[cfg(test)]
mod tests_inspect;
#[cfg(test)]
mod tests_scale;
#[cfg(test)]
mod tests_scope_frame;
#[cfg(test)]
mod tests_tamper;
