//! `Document::check` and `Document::check_pending`: the whole of `stetdoc check` (design `document-and-ops.md` 6).
//!
//! `check` is `check::document` on the current state and reports what is wrong, blocking nothing. `check_pending` is what the
//! command prints: the dry run of the pending files (so the receipt shows the revisions they would write, and a new block has no
//! ref), the revisions the document already holds by author, the report, and the exit status. One function, so that the CLI, napi
//! and a host application cannot disagree about it. It takes `&self` and never installs the state its dry run commits.

use super::Document;
use crate::api::check::CheckReport;
use crate::api::edit::Edit;
use crate::api::error::{ErrorCode, StetError};
use crate::api::files::ReadableFiles;
use crate::api::receipt::{AuthorRevisions, CheckResult};
use crate::api::stamp::Stamp;
use crate::api::view::RevisionFilter;
use crate::check;

/// The exit status of a check that found violations under `strict`: the class of `validation_failed`.
const EXIT_VIOLATION: u8 = 5;

impl Document {
    /// Validates the whole document. It reports and never blocks: a problem that was already there is the answer to a question
    /// that was asked, and every code that occurred is counted.
    pub fn check(&self) -> CheckReport {
        check::document(&self.state.parts())
    }

    /// Everything `stetdoc check` prints, and its exit status.
    ///
    /// With pending files it runs their edit as a dry run (the receipt shows the revisions by author, so the pending files need a
    /// `stamp`, and without one that is `invalid_payload`). An edit that would add a violation is the error `validation_failed`,
    /// whose exit class is 5, and any other failure is an error of its own class. The exit status is 0 when the edit would add
    /// nothing, whatever the document already holds, and with `strict` it is 5 when the report has any violation, new or old.
    /// Nothing here changes the document: `export` is the same before and after.
    pub fn check_pending(
        &self,
        pending: Option<ReadableFiles>,
        stamp: Option<Stamp>,
        strict: bool,
    ) -> Result<CheckResult, StetError> {
        let receipt = match pending {
            None => None,
            Some(files) => {
                let Some(stamp) = stamp else {
                    return Err(StetError::new(
                        ErrorCode::InvalidPayload,
                        "The pending files need the author and the date of the edit, because the receipt of a check shows the revisions by author and date.",
                    )
                    .with_next("Give the stamp that `edit` would take: an author and a date."));
                };
                let (receipt, _next) = self.run_edit(&Edit::files(files, stamp).dry_run())?;
                Some(receipt)
            }
        };
        let report = self.check();
        let revisions = self.revisions(&RevisionFilter::default())?;
        let exit_status = if strict && report.violations > 0 {
            EXIT_VIOLATION
        } else {
            0
        };
        Ok(CheckResult {
            receipt,
            existing: AuthorRevisions::group(&revisions),
            report,
            strict,
            exit_status,
        })
    }
}
