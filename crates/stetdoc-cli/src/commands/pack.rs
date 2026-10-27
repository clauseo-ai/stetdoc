//! `stetdoc pack`: `resume`, the pending files through `edit(Edit::files)`, `save` into the .docx, then `export` so the
//! working copy shows the result and is clean again.

use crate::cli::PackArgs;
use crate::fsio::{self, workdir::Workdir};
use crate::output::{Failure, Outcome, to_json};
use crate::stamp;
use std::path::{Path, PathBuf};
use stetdoc_core::{Edit, ErrorCode, Limits, StetError, WorkingCopy, WriteOptions};

/// Runs `pack`.
pub fn run(args: &PackArgs) -> Result<Outcome, Failure> {
    let (workdir, notes) = Workdir::open(&args.dir)?;
    let (mut doc, disk) = workdir.resume()?;
    let pending = disk.pending_files()?;
    let mut consumed = Vec::new();
    let receipt = match pending {
        Some(files) => {
            consumed = files.files.iter().map(|(n, _)| n.clone()).collect();
            let mut edit = Edit::files(files, stamp::resolve(&args.stamp)?);
            if args.no_track {
                edit = edit.untracked();
            }
            Some(doc.edit(edit)?)
        }
        None => None,
    };
    let saved = doc.save()?;
    let output = args
        .output
        .clone()
        .unwrap_or_else(|| default_output(&args.dir));
    fsio::check_output(&output, &[], &[workdir.dir()])?;
    if !args.force {
        refuse_foreign(&workdir, &disk, &output)?;
    }
    workdir.record_packed(&fsio::sha256_hex(&saved.bytes))?;
    fsio::write_atomic(&output, &saved.bytes)?;
    let next = doc.export()?;
    let options = WriteOptions {
        consumed,
        refresh: false,
    };
    workdir.write(&next, Some(&disk), &options)?;
    let dir = workdir.dir().to_path_buf();
    // Only a pack that wrote and validated the .docx gets here, so the working copy is no longer needed.
    let removal = if args.done {
        Some(workdir.remove(&next))
    } else {
        drop(workdir);
        None
    };

    let mut text = match &receipt {
        Some(receipt) => receipt.text.trim_end().to_string(),
        None => "No hand edits were pending, so the document is written as of its last edit."
            .to_string(),
    };
    text.push_str(&format!("\nwrote {}\n", output.display()));
    let mut notes = notes;
    let removed = match removal {
        None => {
            text.push_str(&format!(
                "working copy {} is at base {}. It stays until you delete the folder, or pack again with --done to remove it.\n",
                dir.display(),
                doc.base()
            ));
            false
        }
        Some(Ok(removal)) => {
            let named = match &removal.real {
                Some(real) => format!(
                    "{} (the folder {} it links to)",
                    dir.display(),
                    real.display()
                ),
                None => dir.display().to_string(),
            };
            if removal.kept.is_empty() {
                text.push_str(&format!("removed the working copy {named}\n"));
            } else {
                text.push_str(&format!(
                    "removed the working copy {named}, except what stetdoc did not write, which stays in the folder: {}\n",
                    removal.kept.join(", ")
                ));
            }
            true
        }
        // The .docx is written, so the pack succeeded: a removal that failed is a warning, not the command's failure.
        Some(Err(Failure::Core(error))) => {
            notes.push(format!(
                "warning: the .docx is written, and the working copy {} could not be removed: {}",
                dir.display(),
                error.message()
            ));
            false
        }
    };
    let json = serde_json::json!({
        "output": output.display().to_string(),
        "receipt": receipt.as_ref().map(to_json).transpose()?,
        "saved": to_json(&saved.receipt)?,
        "working_copy": dir.display().to_string(),
        "working_copy_removed": removed,
    });
    Ok(Outcome::new(text, json).with_notes(notes))
}

/// Refuses an output that exists and holds a document this working copy neither came from nor packed: a newer version that
/// someone else wrote, which a pack would overwrite with an older one (`stale_target`, exit 4).
fn refuse_foreign(workdir: &Workdir, disk: &WorkingCopy, output: &Path) -> Result<(), Failure> {
    let Some(bytes) = fsio::read_file_if_any(output, Limits::default().max_archive_bytes)? else {
        return Ok(());
    };
    let digest = fsio::sha256_hex(&bytes);
    if workdir.own_digests(disk)?.contains(&digest) {
        return Ok(());
    }
    Err(StetError::new(
        ErrorCode::StaleTarget,
        format!(
            "{} exists and is neither the document {} came from nor one it packed, so packing would overwrite a version it has not seen.",
            output.display(),
            workdir.dir().display()
        ),
    )
    .with_next(format!(
        "Unpack {} again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.",
        output.display()
    ))
    .into())
}

/// `contract.stet` packs to `contract.edited.docx` beside it, so a pack with no `-o` never touches the original's path.
pub fn default_output(dir: &Path) -> PathBuf {
    // `.` and `..` have no name of their own, so they are named by where they are.
    let named = match dir.file_name() {
        Some(_) => dir.to_path_buf(),
        None => dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()),
    };
    let dir = named.as_path();
    let stem = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = stem.strip_suffix(".stet").unwrap_or(&stem).to_string();
    let name = format!("{stem}.edited.docx");
    match dir.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}
