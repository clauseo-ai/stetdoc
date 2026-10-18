//! `stetdoc unpack`: `open`, `export`, and the core's write plan executed into a new directory. With `--refresh`, an
//! existing working copy is rendered again.

use crate::cli::UnpackArgs;
use crate::fsio::workdir::Workdir;
use crate::output::{Failure, Outcome};
use crate::target::{self, Source};
use std::path::{Path, PathBuf};
use stetdoc_core::{WorkingCopy, WriteOptions};

/// Runs `unpack`.
pub fn run(args: &UnpackArgs) -> Result<Outcome, Failure> {
    match (Source::of(&args.input), args.refresh) {
        (Source::Docx(path), false) => unpack(&path, args.output.as_deref()),
        (Source::Dir(dir), true) => refresh(&dir),
        (Source::Docx(path), true) => Err(Failure::usage(
            format!(
                "--refresh renders an existing working copy again, and {} is not one.",
                path.display()
            ),
            "Name the working copy's folder, or drop --refresh to unpack the .docx.",
        )),
        (Source::Dir(dir), false) => Err(Failure::usage(
            format!(
                "{} is a folder, and stetdoc unpack reads a .docx.",
                dir.display()
            ),
            "Name the .docx, or add --refresh to render this working copy again, discarding edits that were not packed.",
        )),
    }
}

fn unpack(docx: &Path, output: Option<&Path>) -> Result<Outcome, Failure> {
    let doc = target::open_docx(docx)?;
    let dir = output.map_or_else(|| default_dir(docx), Path::to_path_buf);
    let copy = doc.export()?;
    let existed = dir.exists();
    let workdir = Workdir::create(&dir)?;
    if let Err(failure) = workdir.write(&copy, None, &WriteOptions::default()) {
        workdir.discard(existed);
        return Err(failure);
    }
    Ok(summary(&dir, &copy, &doc.base(), "wrote"))
}

fn refresh(dir: &Path) -> Result<Outcome, Failure> {
    let (workdir, mut notes) = Workdir::open(dir)?;
    notes.extend(workdir.set_aside_undecodable()?);
    let (doc, disk) = workdir.resume()?;
    let copy = doc.export()?;
    let options = WriteOptions {
        consumed: Vec::new(),
        refresh: true,
    };
    workdir.write(&copy, Some(&disk), &options)?;
    Ok(summary(dir, &copy, &doc.base(), "rendered again").with_notes(notes))
}

/// `contract.docx` unpacks to `contract.stet` beside it.
pub fn default_dir(docx: &Path) -> PathBuf {
    docx.with_extension("stet")
}

fn summary(dir: &Path, copy: &WorkingCopy, base: &str, verb: &str) -> Outcome {
    let names: Vec<&str> = copy.files.iter().map(|(n, _)| n.as_str()).collect();
    let text = format!(
        "{verb} {}: {}\nbase {base}\nnext: stetdoc view {}\n",
        dir.display(),
        names.join(", "),
        dir.display()
    );
    let json = serde_json::json!({
        "working_copy": dir.display().to_string(),
        "base": base,
        "files": names,
    });
    Outcome::new(text, json)
}
