//! The envelope of an edit of readable files (design `document-and-ops.md` 3.1 step 2, 3.4 and 3.5): the plan that the planner makes of
//! them, the checks that the files are this release's and not too big, and the failures said in files and lines.
//!
//! An edit of files is a batch whose operations `projection::to_batch` chose, so what follows is the same pipeline as a batch
//! with three differences: the plan can be refused before anything is staged, a failure names the file and the line the agent
//! wrote and not an operation number, and the receipt carries each operation's origin and notes.

use super::Document;
use crate::api::batch::Batch;
use crate::api::error::{ErrorCode, StetError};
use crate::api::files::{Origin, ReadableFiles};
use crate::api::view::Warning;
use crate::ops;
use crate::projection;
use crate::refs::{hex, sha256};
use std::borrow::Cow;
use std::collections::HashSet;

/// What `edit` applies, whichever way it was given.
pub(super) struct Envelope<'a> {
    /// The operations.
    pub batch: Cow<'a, Batch>,
    /// Where each operation came from. Empty for a batch.
    pub origins: Vec<Origin>,
    /// What G said about the plan that is not an error.
    pub notes: Vec<Warning>,
}

impl<'a> Envelope<'a> {
    /// A batch as given.
    pub(super) fn of_batch(batch: &'a Batch) -> Envelope<'a> {
        Envelope {
            batch: Cow::Borrowed(batch),
            origins: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// A failure as the one who wrote it should read it: the file and the line, and the ref the message is about, in place of
    /// the number of an operation that the agent never wrote.
    pub(super) fn locate(&self, error: StetError) -> StetError {
        let Some(origin) = error.op_index().and_then(|i| self.origins.get(i)) else {
            return error;
        };
        let message = match &origin.block {
            Some(shown) if !error.message().starts_with(shown.as_str()) => {
                format!("{shown}: {}", error.message())
            }
            _ => error.message().to_string(),
        };
        StetError::new(error.code(), message)
            .with_next(error.next())
            .at_line(origin.file.clone(), origin.line.max(1))
    }
}

fn stale(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::StaleFiles, message).with_next(
        "Export the document again, and make the edit again on the new files. Nothing was applied.",
    )
}

/// The refusals that a release which renders differently would cause for every difference: a changed tag, a changed head, a
/// changed line would each look like an edit of a read-only region.
fn could_be_skew(error: &StetError) -> bool {
    matches!(
        error.code(),
        ErrorCode::ReadonlyRegion
            | ErrorCode::InvalidPayload
            | ErrorCode::Unsupported
            | ErrorCode::UnsupportedVersion
    )
}

/// Checks the hashes recorded when the files were written against what this release renders for the same state. A file that
/// has a recorded hash and no longer renders is as stale as one that renders differently.
fn check_skew(recorded: &[(String, String)], now: &[(String, [u8; 32])]) -> Result<(), StetError> {
    for (name, was) in recorded {
        let same = now
            .iter()
            .find(|(n, _)| n == name)
            .is_some_and(|(_, h)| hex(h).eq_ignore_ascii_case(was));
        if !same {
            return Err(stale(format!(
                "`{name}` was written by another release of stetdoc: it renders differently now, so every difference would look like an edit."
            )));
        }
    }
    Ok(())
}

/// How many blocks the plan changes: the distinct blocks that its operations name, and every operation that names none.
fn changed_blocks(origins: &[Origin]) -> usize {
    let mut named: HashSet<&str> = HashSet::new();
    let mut anonymous = 0usize;
    for origin in origins {
        match origin.block.as_deref() {
            Some(block) => {
                named.insert(block);
            }
            None => anonymous += 1,
        }
        if let Some(also) = origin.also.as_deref() {
            named.insert(also);
        }
    }
    named.len() + anonymous
}

impl Document {
    /// What an edit of these files comes to, ready to apply.
    pub(super) fn plan_files(&self, files: &ReadableFiles) -> Result<Envelope<'static>, StetError> {
        let planned = match projection::to_batch(&self.state, files, &self.limits) {
            Ok(planned) => planned,
            Err(error) => {
                if could_be_skew(&error) && !files.rendered.is_empty() {
                    let now = projection::render_files(&self.state, &files.format)?;
                    let hashes: Vec<(String, [u8; 32])> = now
                        .files
                        .iter()
                        .map(|(n, text)| (n.clone(), sha256(text.as_bytes())))
                        .collect();
                    check_skew(&files.rendered, &hashes)?;
                }
                return Err(error);
            }
        };
        check_skew(&files.rendered, &planned.baseline)?;
        let blocks = changed_blocks(&planned.origins);
        if blocks as u64 > u64::from(self.limits.max_batch_ops) {
            return Err(StetError::new(
                ErrorCode::LimitExceeded,
                format!(
                    "The edit changes {blocks} blocks, over the limit of {}.",
                    self.limits.max_batch_ops
                ),
            )
            .with_next(
                "Apply the edit in parts: keep some of the changes, save and export, then make the rest.",
            ));
        }
        let batch = ops::planned_batch(&files.base, &planned);
        if !batch.ops.is_empty() {
            // The limit on operations counted the blocks above, and an operation here is not a block.
            let mut limits = self.limits.clone();
            limits.max_batch_ops = u32::MAX;
            ops::validate_batch(&batch, &limits)?;
        }
        Ok(Envelope {
            batch: Cow::Owned(batch),
            origins: planned.origins,
            notes: planned.notes,
        })
    }
}
