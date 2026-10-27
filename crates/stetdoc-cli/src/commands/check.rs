//! `stetdoc check`: `check_pending`, the one function behind the command, and its exit status.

use crate::cli::CheckArgs;
use crate::fsio::workdir::Workdir;
use crate::output::{Failure, Outcome, to_json};
use crate::stamp;
use crate::target::{self, Source};
use std::fmt::Write;
use stetdoc_core::{AuthorRevisions, CheckResult};

/// How many findings the text lists without `--all`. `--all` and `--json` have all that the report keeps.
const SHOWN_FINDINGS: usize = 20;

/// Runs `check`.
pub fn run(args: &CheckArgs) -> Result<Outcome, Failure> {
    match Source::of(&args.input)? {
        Source::Docx(path) => {
            let doc = target::open_docx(&path)?;
            outcome(&doc.check_pending(None, None, args.strict)?, args.all)
        }
        Source::Dir(path) => {
            let (workdir, notes) = Workdir::open(&path)?;
            let (doc, copy) = workdir.resume()?;
            let pending = copy.pending_files()?;
            // A stamp is needed only for pending files, whose receipt shows the revisions by author and date.
            let stamp = match pending {
                Some(_) => Some(stamp::resolve(&args.stamp)?),
                None => None,
            };
            let result = doc.check_pending(pending, stamp, args.strict)?;
            Ok(outcome(&result, args.all)?.with_notes(notes))
        }
    }
}

/// What `check` prints for a result, with its exit status.
fn outcome(result: &CheckResult, all: bool) -> Result<Outcome, Failure> {
    let mut outcome = Outcome::new(text(result, all), to_json(result)?);
    outcome.exit = result.exit_status;
    Ok(outcome)
}

fn text(result: &CheckResult, all: bool) -> String {
    let mut out = String::new();
    match &result.receipt {
        Some(receipt) => {
            let _ = writeln!(out, "Pending edits, as pack would write them:");
            let _ = writeln!(out, "{}", receipt.text.trim_end());
        }
        None => {
            let _ = writeln!(out, "No pending edits.");
        }
    }
    if !result.existing.is_empty() {
        let _ = writeln!(out, "\nRevisions already in the document:");
        for author in &result.existing {
            let _ = writeln!(out, "  {}: {}", author.author, counts(author));
        }
    }
    let report = &result.report;
    let _ = writeln!(out);
    if report.ok {
        let _ = write!(
            out,
            "Validation: no violations in {}",
            plural(report.parts, "part")
        );
    } else {
        let _ = write!(
            out,
            "Validation: {} in {}, already in the document and not caused by any edit",
            plural(report.violations, "violation"),
            plural(report.parts, "part")
        );
    }
    if report.notes > 0 {
        let _ = write!(out, ", {}", plural(report.notes, "note"));
    }
    let _ = writeln!(out, ".");
    let shown = if all {
        report.findings.len()
    } else {
        SHOWN_FINDINGS
    };
    for finding in report.findings.iter().take(shown) {
        let place = match &finding.block {
            Some(block) => format!("{} {block}", finding.part),
            None => finding.part.clone(),
        };
        let _ = writeln!(
            out,
            "  {} ({place}): {}",
            finding.code.as_str(),
            finding.message
        );
    }
    let listed = report.findings.len().min(shown);
    let counted = report.violations + report.notes;
    if report.findings.len() > listed {
        let _ = writeln!(
            out,
            "  ... {} more; --all lists them.",
            report.findings.len() - listed
        );
    } else if counted > listed {
        // The report keeps the first findings and counts the rest, by code in `--json`.
        let _ = writeln!(
            out,
            "  ... {} more, which the report counts by code (--json) and does not list.",
            counted - listed
        );
    }
    if result.strict && result.exit_status != 0 {
        let _ = writeln!(
            out,
            "--strict: the document has violations, so the check fails."
        );
    }
    out
}

/// `1 part`, `2 parts`.
fn plural(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

/// `3 insertion, 1 deletion`.
fn counts(author: &AuthorRevisions) -> String {
    author
        .counts
        .iter()
        .map(|(kind, n)| {
            let kind = serde_json::to_value(kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
                .replace('_', " ");
            format!("{n} {kind}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}
