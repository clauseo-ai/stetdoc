//! The only file I/O in the project: bounded reads and atomic writes. A working copy's directory is `workdir`'s.

pub mod workdir;

use crate::output::Failure;
use std::fs::{self, File};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use stetdoc_core::{ErrorCode, StetError};

/// Reads a whole file of at most `max` bytes. A missing file is `not_found`, a larger one `limit_exceeded`.
pub fn read_file(path: &Path, max: u64) -> Result<Vec<u8>, Failure> {
    let meta = fs::metadata(path).map_err(|e| missing(path, &e))?;
    if meta.is_dir() {
        return Err(StetError::new(
            ErrorCode::InvalidPayload,
            format!(
                "{} is a directory, and a file was expected.",
                path.display()
            ),
        )
        .with_next("Name a .docx file.")
        .into());
    }
    if meta.len() > max {
        return Err(too_big(path, max).into());
    }
    let file = File::open(path).map_err(|e| missing(path, &e))?;
    read_bounded(file, max, path)
}

/// Reads at most `max` bytes, and refuses more.
pub fn read_bounded(reader: impl Read, max: u64, path: &Path) -> Result<Vec<u8>, Failure> {
    let mut bytes = Vec::new();
    reader
        .take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| missing(path, &e))?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(too_big(path, max).into());
    }
    Ok(bytes)
}

/// The text of a batch: from `--ops`, or from stdin when the flag is absent or `-`.
pub fn read_batch(ops: Option<&Path>, max: u64) -> Result<String, Failure> {
    let bytes = match ops {
        Some(path) if path != Path::new("-") => read_file(path, max)?,
        _ => {
            let stdin = std::io::stdin();
            if stdin.is_terminal() {
                return Err(Failure::usage(
                    "No batch: stetdoc reads the operations from stdin or from --ops, and stdin is a terminal.",
                    "Pass the batch with a heredoc (stetdoc edit DIR <<'EOF' ... EOF) or with --ops FILE.",
                ));
            }
            read_bounded(stdin.lock(), max, Path::new("stdin"))?
        }
    };
    String::from_utf8(bytes).map_err(|_| {
        Failure::usage(
            "The batch is not UTF-8 text.",
            "Write the batch as UTF-8 block text or JSON.",
        )
    })
}

/// Writes a file atomically: a temporary file beside it, an fsync, a rename, and an fsync of the directory. A crash leaves
/// the old file or the new one, never half of one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    let dir = parent(path);
    let tmp = temp_path(path);
    let result = (|| -> std::io::Result<()> {
        // A new file, never one that is there: a leftover of a crashed process with the same id is removed first, and a
        // link planted under the temporary name is never followed.
        let _ = fs::remove_file(&tmp);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        sync_dir(&dir)
    })();
    result.map_err(|e| {
        let _ = fs::remove_file(&tmp);
        write_failed(path, &e)
    })
}

/// The temporary file a write of `path` goes through: hidden, beside it, and with no readable extension, so a stray one
/// is never taken for a readable file or a complement entry.
pub fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    parent(path).join(format!(".{name}.{}.tmp", std::process::id()))
}

/// Refuses an output that is one of the command's inputs, or a path inside a working copy: a write there would destroy
/// what the command reads, or plant a file in a folder that stetdoc manages.
pub fn check_output(
    output: &Path,
    inputs: &[&Path],
    working_copies: &[&Path],
) -> Result<(), Failure> {
    let target = resolved(output);
    for input in inputs {
        if resolved(input) == target {
            return Err(Failure::usage(
                format!("-o {} names an input of this command.", output.display()),
                "Write the result to another file.",
            ));
        }
    }
    for dir in working_copies {
        if target.starts_with(resolved(dir)) {
            return Err(Failure::usage(
                format!(
                    "-o {} is inside the working copy {}.",
                    output.display(),
                    dir.display()
                ),
                "Write the .docx outside the working copy, for example beside it.",
            ));
        }
    }
    Ok(())
}

/// A path with its links and `..` resolved, as far as it exists: the folder of a file that is not there yet is resolved,
/// and the file's name is kept.
fn resolved(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => resolved(if dir.as_os_str().is_empty() {
            Path::new(".")
        } else {
            dir
        })
        .join(name),
        _ => path.to_path_buf(),
    }
}

/// The SHA-256 of some bytes, in lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Creates a directory and its parents.
pub fn ensure_dir(dir: &Path) -> Result<(), Failure> {
    fs::create_dir_all(dir).map_err(|e| write_failed(dir, &e))
}

/// The bytes of a file, or `None` when there is none.
pub fn read_file_if_any(path: &Path, max: u64) -> Result<Option<Vec<u8>>, Failure> {
    if !path.exists() {
        return Ok(None);
    }
    read_file(path, max).map(Some)
}

/// The text of a file, or `None` when there is none. A file that is there and cannot be read is an error.
pub fn read_text_if_any(path: &Path, max: u64) -> Result<Option<String>, Failure> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = read_file(path, max)?;
    String::from_utf8(bytes).map(Some).map_err(|_| {
        Failure::usage(
            format!("{} is not UTF-8 text.", path.display()),
            "Save it as UTF-8, then run the command again.",
        )
    })
}

/// The names of the entries of a directory.
pub fn list_dir(dir: &Path) -> Result<Vec<String>, Failure> {
    let entries = fs::read_dir(dir).map_err(|e| {
        Failure::io(
            format!("Could not read {}: {e}.", dir.display()),
            "Check the folder's permissions, then run the command again.",
        )
    })?;
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| {
            Failure::io(
                format!("Could not read {}: {e}.", dir.display()),
                "Check the folder's permissions, then run the command again.",
            )
        })?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

/// Removes a file.
pub fn remove_file(path: &Path) -> Result<(), Failure> {
    fs::remove_file(path).map_err(|e| write_failed(path, &e))
}

/// Removes an empty directory. A directory that is not empty stays, and that is an error.
pub fn remove_empty_dir(dir: &Path) -> Result<(), Failure> {
    fs::remove_dir(dir).map_err(|e| write_failed(dir, &e))
}

/// Whether a file name is one of the temporary files of `write_atomic`.
pub fn is_temp_name(name: &str) -> bool {
    name.starts_with('.') && name.ends_with(".tmp")
}

/// The directory a path is in, `.` for a bare name.
pub fn parent(path: &Path) -> PathBuf {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Makes a rename in `dir` durable. Only Unix can open a directory to sync it.
pub fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()?;
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
    Ok(())
}

/// A write that failed.
pub fn write_failed(path: &Path, error: &std::io::Error) -> Failure {
    Failure::io(
        format!("Could not write {}: {error}.", path.display()),
        "Check that the folder exists and is writable, and that the disk has space, then run the command again.",
    )
}

fn missing(path: &Path, error: &std::io::Error) -> Failure {
    // The OS's wording differs between platforms, so the common case has a sentence of its own.
    let message = if error.kind() == std::io::ErrorKind::NotFound {
        format!("There is no file at {}.", path.display())
    } else {
        format!("Could not read {}: {error}.", path.display())
    };
    StetError::new(ErrorCode::NotFound, message)
        .with_next("Check the path, and that the file is readable.")
        .into()
}

fn too_big(path: &Path, max: u64) -> StetError {
    StetError::new(
        ErrorCode::LimitExceeded,
        format!(
            "{} is larger than the limit of {max} bytes.",
            path.display()
        ),
    )
    .with_next("Split the input, or ask whoever runs stetdoc to raise the limit.")
}
