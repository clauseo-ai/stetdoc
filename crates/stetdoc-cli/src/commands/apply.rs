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
    /// Write nothing. A dry run of a .docx needs neither -o nor --in-place, and one that has either accepts it and ignores
    /// it, so that no pair of hints can send an agent round in a loop.
    pub dry_run: bool,
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
    match Source::of(at.input)? {
        Source::Docx(path) => apply_docx(at, &path, stamp, make),
        Source::Dir(path) => apply_dir(at, &path, stamp, make),
    }
}

/// Joins two head lines, one per line, when there are any.
fn join_head(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(a), Some(b)) => Some(format!("{a}\n{b}")),
        (a, b) => a.or(b),
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
    let author = stamp.author.clone();
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
    let outcome = tell_others(outcome, &doc, &author, dir)?;
    Ok(finish(outcome, made.head, made.extra))
}

/// Where an edit of a .docx writes: another file, the file stetdoc names beside the input, the input, or nothing, for a dry
/// run.
enum Target {
    File(PathBuf),
    Default(PathBuf),
    InPlace,
    Nothing,
}

/// `contract.docx` is edited into `contract.edited.docx` beside it, as `pack` names its output, so an edit with neither -o nor
/// --in-place never touches the input. When that file exists the edit goes to `contract.edited-2.docx`, then `-3`, and an
/// input that is one of these names takes the next free one, so a chain of edits reads in order and nothing is overwritten.
pub fn default_output(input: &Path) -> PathBuf {
    (1..10_000)
        .map(|n| edited_name(input, n))
        .find(|candidate| {
            candidate.symlink_metadata().is_err() && !fsio::same_file(candidate, input)
        })
        .unwrap_or_else(|| edited_name(input, 10_000))
}

/// The `n`th name of an edit of `input`: `contract.edited.docx` for 1, `contract.edited-2.docx` for 2, from `contract.docx` or
/// from any of these names.
fn edited_name(input: &Path, n: u32) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let base = match stem.rsplit_once(".edited") {
        Some((base, "")) => base.to_string(),
        Some((base, tail))
            if tail
                .strip_prefix('-')
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())) =>
        {
            base.to_string()
        }
        _ => stem,
    };
    let extension = input
        .extension()
        .map_or_else(|| "docx".to_string(), |e| e.to_string_lossy().into_owned());
    let name = if n == 1 {
        format!("{base}.edited.{extension}")
    } else {
        format!("{base}.edited-{n}.{extension}")
    };
    input.with_file_name(name)
}

/// The line that says a dry run ignored -o or --in-place, or `None` when it had neither.
fn ignored_output(at: &Where<'_>) -> Option<String> {
    if !at.dry_run {
        return None;
    }
    let flags = match (at.output.is_some(), at.in_place) {
        (true, true) => "-o and --in-place are",
        (true, false) => "-o is",
        (false, true) => "--in-place is",
        (false, false) => return None,
    };
    Some(format!("--dry-run writes nothing, so {flags} ignored."))
}

fn docx_target(at: &Where<'_>, path: &Path) -> Result<Target, Failure> {
    if at.dry_run {
        return Ok(Target::Nothing);
    }
    match (at.output, at.in_place) {
        (Some(_), true) => Err(Failure::usage(
            "-o and --in-place both say where to write, and an edit writes one .docx.",
            "Pass -o to write another file, or --in-place to write back to the input.",
        )),
        (None, true) => Ok(Target::InPlace),
        (Some(output), false) => {
            if fsio::same_file(output, path) {
                return Err(Failure::usage(
                    format!("-o {} names the input of this edit.", output.display()),
                    "Use --in-place to write back to the same file, or name another file with -o.",
                ));
            }
            fsio::check_output(output, &[path], &[])?;
            Ok(Target::File(output.clone()))
        }
        (None, false) => {
            let output = default_output(path);
            fsio::check_output(&output, &[path], &[])?;
            Ok(Target::Default(output))
        }
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
    let author = stamp.author.clone();
    let options = OpenOptions::default();
    let input = fsio::read_file(path, options.limits.max_archive_bytes)?;
    let digest = fsio::sha256_hex(&input);
    let mut doc = Document::open(input, &options)?;
    let made = make(&doc, stamp)?;
    let receipt = doc.edit(made.edit)?;
    let written = match target {
        Target::Nothing => {
            let head = join_head(ignored_output(at), made.head);
            let outcome = tell_others(Outcome::of(&receipt.text, &receipt)?, &doc, &author, path)?;
            return Ok(finish(outcome, head, made.extra));
        }
        Target::File(output) => (output, None),
        Target::Default(output) => {
            let said = defaulted(path, &output);
            (output, Some(said))
        }
        Target::InPlace => (path.to_path_buf(), None),
    };
    let (written, defaulted) = written;
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
        "default_output": defaulted.is_some(),
        "receipt": told.receipt,
        "saved": to_json(&saved.receipt)?,
        "next": told.next,
    });
    let mut outcome = Outcome::new(told.text, json);
    if let Some(said) = defaulted {
        outcome.text.push_str(&said);
    }
    let outcome = tell_others(outcome, &doc, &author, &written)?;
    Ok(finish(outcome, made.head, made.extra))
}

/// The line after `wrote` when stetdoc named the output: the input is unchanged, and where the next edit goes.
fn defaulted(input: &Path, output: &Path) -> String {
    let first = edited_name(input, 1);
    let skipped = if output == first || fsio::same_file(input, &first) {
        String::new()
    } else {
        format!(
            " {} already exists, so this edit went to a new file, which does not have the edits in {}.",
            first.display(),
            first.display()
        )
    };
    format!(
        "{} is unchanged.{skipped} Run the next edit on {} to build on this one, or pass -o FILE or --in-place to choose.\n",
        input.display(),
        output.display()
    )
}

/// Adds the line about tracked changes that authors other than `author` made, when there are any, to the text and the JSON.
pub fn tell_others(
    mut outcome: Outcome,
    doc: &Document,
    author: &str,
    file: &Path,
) -> Result<Outcome, Failure> {
    if let Some(others) = super::others::others(doc, Some(author), file)? {
        if !outcome.text.ends_with('\n') {
            outcome.text.push('\n');
        }
        outcome.text.push_str(&others.text);
        if let Some(object) = outcome.json.as_object_mut() {
            object.insert("others".to_string(), others.json);
        }
    }
    Ok(outcome)
}
