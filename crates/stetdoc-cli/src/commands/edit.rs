//! `stetdoc edit`: a batch from stdin or `--ops`, applied with `edit(Edit::ops)`, to a working copy or to a package.

use crate::cli::{EditArgs, OffsetsArg, QuotesArg};
use crate::fsio;
use crate::fsio::workdir::Workdir;
use crate::output::{Failure, Outcome, to_json};
use crate::stamp;
use crate::target::Source;
use stetdoc_core::{
    Batch, Document, Edit, Limits, OffsetUnit, OpenOptions, Quotes, Stamp, WriteOptions,
};

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
    if args.in_place {
        return Err(Failure::usage(
            format!(
                "--in-place writes a .docx back to itself, and {} is a working copy, which an edit always writes.",
                dir.display()
            ),
            "Drop --in-place, and run stetdoc pack after the edit to write the .docx.",
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
    let output = match (&args.output, args.in_place) {
        (Some(_), true) => {
            return Err(Failure::usage(
                "-o and --in-place both say where to write, and an edit writes one .docx.",
                "Pass -o to write another file, or --in-place to write back to the input.",
            ));
        }
        (None, true) if args.dry_run => {
            return Err(Failure::usage(
                "--dry-run writes nothing, and --in-place says to write the input.",
                "Drop --in-place to see the receipt, then run the edit again with --in-place and without --dry-run.",
            ));
        }
        (None, true) => None,
        (Some(output), false) => {
            if fsio::same_file(output, path) {
                return Err(Failure::usage(
                    format!("-o {} names the input of this edit.", output.display()),
                    "Use --in-place to write back to the same file, or name another file with -o.",
                ));
            }
            fsio::check_output(output, &[path], &[])?;
            Some(output.clone())
        }
        (None, false) => {
            return Err(Failure::usage(
                format!(
                    "{} is a .docx, and an edit of a .docx needs to know where to write.",
                    path.display()
                ),
                "Name another file with -o, pass --in-place to write back to this one, or unpack the document and edit the working copy.",
            ));
        }
    };
    let stamp = stamp::resolve(&args.stamp)?;
    let options = OpenOptions::default();
    let input = fsio::read_file(path, options.limits.max_archive_bytes)?;
    let digest = fsio::sha256_hex(&input);
    let mut doc = Document::open(input, &options)?;
    let edit = build(args, stamp)?;
    let receipt = doc.edit(edit)?;
    if args.dry_run {
        return Outcome::of(&receipt.text, &receipt);
    }
    let saved = doc.save()?;
    let written = match &output {
        Some(output) => {
            fsio::write_atomic(output, &saved.bytes)?;
            output.clone()
        }
        None => {
            fsio::replace_atomic(path, &saved.bytes, &digest)?;
            path.to_path_buf()
        }
    };
    let told = super::written::retell(&doc, &receipt, &saved.bytes, &written, None)?;
    let json = serde_json::json!({
        "output": written.display().to_string(),
        "in_place": output.is_none(),
        "receipt": told.receipt,
        "saved": to_json(&saved.receipt)?,
        "next": told.next,
    });
    Ok(Outcome::new(told.text, json))
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
