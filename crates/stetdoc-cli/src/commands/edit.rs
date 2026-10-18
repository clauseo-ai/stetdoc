//! `stetdoc edit`: a batch from stdin or `--ops`, applied with `edit(Edit::ops)`, to a working copy or to a package.

use crate::cli::{EditArgs, OffsetsArg, QuotesArg};
use crate::fsio;
use crate::fsio::workdir::Workdir;
use crate::output::{Failure, Outcome, to_json};
use crate::stamp;
use crate::target::{self, Source};
use stetdoc_core::{Batch, Edit, Limits, OffsetUnit, Quotes, Stamp, WriteOptions};

/// Runs `edit`.
pub fn run(args: &EditArgs) -> Result<Outcome, Failure> {
    match Source::of(&args.input) {
        Source::Docx(path) => edit_docx(args, &path),
        Source::Dir(path) => edit_dir(args, &path),
    }
}

/// A batch on a working copy: refused while hand edits are pending, so that file edits and batches never mix in one step.
fn edit_dir(args: &EditArgs, dir: &std::path::Path) -> Result<Outcome, Failure> {
    if args.output.is_some() {
        return Err(Failure::usage(
            "-o names the .docx to write when editing a .docx. A working copy is edited in place, and stetdoc pack writes its .docx.",
            "Drop -o, and run stetdoc pack after the edit.",
        ));
    }
    let stamp = stamp::resolve(&args.stamp)?;
    let (workdir, notes) = Workdir::open(dir)?;
    let (mut doc, disk) = workdir.resume()?;
    disk.ensure_clean()?;
    let receipt = doc.edit(build(args, stamp)?)?;
    if !args.dry_run {
        let next = doc.export()?;
        workdir.write(&next, Some(&disk), &WriteOptions::default())?;
    }
    Ok(Outcome::of(&receipt.text, &receipt)?.with_notes(notes))
}

fn edit_docx(args: &EditArgs, path: &std::path::Path) -> Result<Outcome, Failure> {
    let Some(output) = &args.output else {
        return Err(Failure::usage(
            format!(
                "{} is a .docx, and stetdoc never edits a package in place.",
                path.display()
            ),
            "Name the file to write with -o, or unpack the document and edit the working copy.",
        ));
    };
    fsio::check_output(output, &[path], &[])?;
    let stamp = stamp::resolve(&args.stamp)?;
    let mut doc = target::open_docx(path)?;
    let edit = build(args, stamp)?;
    let receipt = doc.edit(edit)?;
    if args.dry_run {
        return Outcome::of(&receipt.text, &receipt);
    }
    let saved = doc.save()?;
    fsio::write_atomic(output, &saved.bytes)?;
    let json = serde_json::json!({
        "output": output.display().to_string(),
        "receipt": to_json(&receipt)?,
        "saved": to_json(&saved.receipt)?,
    });
    let text = format!("{}\nwrote {}\n", receipt.text.trim_end(), output.display());
    Ok(Outcome::new(text, json))
}

/// The `Edit` the flags and the batch ask for.
pub fn build(args: &EditArgs, stamp: Stamp) -> Result<Edit, Failure> {
    let limits = Limits::default();
    let text = match &args.batch {
        Some(text) => text.clone(),
        None => fsio::read_batch(args.ops.as_deref(), limits.max_batch_bytes)?,
    };
    let batch = Batch::parse(&text, &limits)?;
    let mut edit = Edit::ops(batch, stamp)
        .quotes(match args.quotes {
            QuotesArg::Match => Quotes::Match,
            QuotesArg::AsTyped => Quotes::AsTyped,
        })
        .offsets(match args.offsets {
            OffsetsArg::Scalar => OffsetUnit::Scalar,
            OffsetsArg::Utf16 => OffsetUnit::Utf16,
        });
    if args.no_track {
        edit = edit.untracked();
    }
    let scope: Vec<String> = args
        .scope
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !scope.is_empty() {
        edit = edit.scope(scope);
    }
    if args.dry_run {
        edit = edit.dry_run();
    }
    Ok(edit)
}
