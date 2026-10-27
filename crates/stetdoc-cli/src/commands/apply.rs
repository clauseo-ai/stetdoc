//! Applying one `Edit` to a working copy or a .docx and writing the result: what `edit`, `replace` and `comment` share.
//!
//! The command builds its `Edit` from the document as it is opened here, so a command that finds its targets itself
//! (`replace`, `comment`) reads and writes one state, and nothing can change between the two.

use crate::cli::StampArgs;
use crate::fsio;
use crate::fsio::workdir::Workdir;
use crate::output::{Failure, Outcome, to_json};
use crate::stamp;
use crate::target::Source;
use serde_json::Value;
use std::path::{Path, PathBuf};
use stetdoc_core::{Document, Edit, OpenOptions, Stamp, WriteOptions};

/// Where an edit reads and writes.
pub struct Where<'a> {
    /// A working copy, or a .docx.
    pub input: &'a Path,
    /// With a .docx, the .docx to write.
    pub output: Option<&'a PathBuf>,
    /// With a .docx, write back to it.
    pub in_place: bool,
    /// Write nothing.
    pub dry_run: bool,
    /// A dry run of a .docx still names where it would write, as `edit` has always asked. `replace` and `comment` take a
    /// dry run with or without -o or --in-place.
    pub dry_run_needs_output: bool,
}

/// What a command made from the opened document.
pub struct Made {
    /// The edit.
    pub edit: Edit,
    /// Lines to print before the receipt, such as which places were found.
    pub head: Option<String>,
    /// More fields for the JSON, such as the places found.
    pub extra: Vec<(&'static str, Value)>,
}

impl Made {
    /// An edit with nothing to add.
    pub fn plain(edit: Edit) -> Made {
        Made {
            edit,
            head: None,
            extra: Vec::new(),
        }
    }
}

/// Checks the flags, resolves the stamp, opens the input, makes the edit with `make`, applies it and writes the result.
pub fn apply(
    at: &Where<'_>,
    stamp: &StampArgs,
    make: impl FnOnce(&Document, Stamp) -> Result<Made, Failure>,
) -> Result<Outcome, Failure> {
    match Source::of(at.input) {
        Source::Docx(path) => apply_docx(at, &path, stamp, make),
        Source::Dir(path) => apply_dir(at, &path, stamp, make),
    }
}

/// Adds the head lines and the extra fields to an outcome.
fn finish(
    mut outcome: Outcome,
    head: Option<String>,
    extra: Vec<(&'static str, Value)>,
) -> Outcome {
    if let Some(head) = head {
        outcome.text = format!("{}\n{}", head.trim_end(), outcome.text);
    }
    if let Some(object) = outcome.json.as_object_mut() {
        for (key, value) in extra {
            object.insert(key.to_string(), value);
        }
    }
    outcome
}

/// An edit of a working copy: refused while hand edits are pending, so that file edits and batches never mix in one step.
fn apply_dir(
    at: &Where<'_>,
    dir: &Path,
    stamp: &StampArgs,
    make: impl FnOnce(&Document, Stamp) -> Result<Made, Failure>,
) -> Result<Outcome, Failure> {
    if at.output.is_some() {
        return Err(Failure::usage(
            "-o names the .docx to write when editing a .docx. A working copy is edited in place, and stetdoc pack writes its .docx.",
            "Drop -o, and run stetdoc pack after the edit.",
        ));
    }
    if at.in_place {
        return Err(Failure::usage(
            format!(
                "--in-place writes a .docx back to itself, and {} is a working copy, which an edit always writes.",
                dir.display()
            ),
            "Drop --in-place, and run stetdoc pack after the edit to write the .docx.",
        ));
    }
    let stamp = stamp::resolve(stamp)?;
    let (workdir, notes) = Workdir::open(dir)?;
    let (mut doc, disk) = workdir.resume()?;
    disk.ensure_clean()?;
    let made = make(&doc, stamp)?;
    let receipt = doc.edit(made.edit)?;
    if !at.dry_run {
        let next = doc.export()?;
        workdir.write(&next, Some(&disk), &WriteOptions::default())?;
    }
    let outcome = Outcome::of(&receipt.text, &receipt)?.with_notes(notes);
    Ok(finish(outcome, made.head, made.extra))
}

/// Where an edit of a .docx writes: another file, the input (`None`), or, for a dry run, nothing.
enum Target {
    File(PathBuf),
    InPlace,
    Nothing,
}

fn docx_target(at: &Where<'_>, path: &Path) -> Result<Target, Failure> {
    match (at.output, at.in_place) {
        (Some(_), true) => Err(Failure::usage(
            "-o and --in-place both say where to write, and an edit writes one .docx.",
            "Pass -o to write another file, or --in-place to write back to the input.",
        )),
        (None, true) if at.dry_run && at.dry_run_needs_output => Err(Failure::usage(
            "--dry-run writes nothing, and --in-place says to write the input.",
            "Drop --in-place to see the receipt, then run the edit again with --in-place and without --dry-run.",
        )),
        (_, _) if at.dry_run && !at.dry_run_needs_output => Ok(Target::Nothing),
        (None, true) => Ok(Target::InPlace),
        (Some(output), false) => {
            if fsio::same_file(output, path) {
                return Err(Failure::usage(
                    format!("-o {} names the input of this edit.", output.display()),
                    "Use --in-place to write back to the same file, or name another file with -o.",
                ));
            }
            fsio::check_output(output, &[path], &[])?;
            Ok(if at.dry_run {
                Target::Nothing
            } else {
                Target::File(output.clone())
            })
        }
        (None, false) => Err(Failure::usage(
            format!(
                "{} is a .docx, and an edit of a .docx needs to know where to write.",
                path.display()
            ),
            "Name another file with -o, pass --in-place to write back to this one, or unpack the document and edit the working copy.",
        )),
    }
}

fn apply_docx(
    at: &Where<'_>,
    path: &Path,
    stamp: &StampArgs,
    make: impl FnOnce(&Document, Stamp) -> Result<Made, Failure>,
) -> Result<Outcome, Failure> {
    let target = docx_target(at, path)?;
    let stamp = stamp::resolve(stamp)?;
    let options = OpenOptions::default();
    let input = fsio::read_file(path, options.limits.max_archive_bytes)?;
    let digest = fsio::sha256_hex(&input);
    let mut doc = Document::open(input, &options)?;
    let made = make(&doc, stamp)?;
    let receipt = doc.edit(made.edit)?;
    let written = match target {
        Target::Nothing => {
            return Ok(finish(
                Outcome::of(&receipt.text, &receipt)?,
                made.head,
                made.extra,
            ));
        }
        Target::File(output) => output,
        Target::InPlace => path.to_path_buf(),
    };
    let saved = doc.save()?;
    if written == path {
        fsio::replace_atomic(path, &saved.bytes, &digest)?;
    } else {
        fsio::write_atomic(&written, &saved.bytes)?;
    }
    let told = super::written::retell(&doc, &receipt, &saved.bytes, &written, None)?;
    let json = serde_json::json!({
        "output": written.display().to_string(),
        "in_place": written == path,
        "receipt": told.receipt,
        "saved": to_json(&saved.receipt)?,
        "next": told.next,
    });
    Ok(finish(Outcome::new(told.text, json), made.head, made.extra))
}
