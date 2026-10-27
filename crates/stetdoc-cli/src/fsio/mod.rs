//! The only file I/O in the project: bounded reads and atomic writes. A working copy's directory is `workdir`'s.

pub mod workdir;

use crate::output::Failure;
use std::fs::{self, File};
use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use stetdoc_core::{ErrorCode, StetError};

/// Refuses a path that is not there, with the message `view` gives for a missing file. A command checks its inputs with this
/// before it sniffs their type, so a missing path is never reported as the wrong kind of file.
pub fn present(path: &Path) -> Result<(), Failure> {
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(missing(path, &e)),
        _ => Ok(()),
    }
}

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
/// the old file or the new one, never half of one. Only Unix can sync a directory, so elsewhere the rename is as durable as
/// the file system makes it.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    write_through_temp(path, path, bytes, None, || Ok(()))
}

/// Writes a file back over itself, as `write_atomic` does. A link is followed, so the file it names is written and the link
/// stays; a hard link is broken, because the rename puts a new file at the path; the permissions are kept and the owner is
/// not (the new file belongs to whoever runs stetdoc). The temporary file is created readable by its owner only, and gets
/// the target's permissions just before the rename. A read-only file is refused rather than replaced. `expected` is the
/// SHA-256 of the bytes the command read: when the file holds anything else just before the rename, someone wrote it during
/// the command, and the write is refused (`stale_target`, exit 4) with the file untouched. Messages name `path` as given.
pub fn replace_atomic(path: &Path, bytes: &[u8], expected: &str) -> Result<(), Failure> {
    let target = path.canonicalize().map_err(|e| missing(path, &e))?;
    let permissions = fs::metadata(&target)
        .map_err(|e| missing(path, &e))?
        .permissions();
    if permissions.readonly() {
        return Err(Failure::io(
            format!(
                "{} is read-only, so it is not written in place.",
                path.display()
            ),
            "Make the file writable, or write the result to another file with -o.",
        ));
    }
    write_through_temp(&target, path, bytes, Some(permissions), || {
        let now = read_file(&target, stetdoc_core::Limits::default().max_archive_bytes)?;
        if sha256_hex(&now) == expected {
            return Ok(());
        }
        Err(StetError::new(
            ErrorCode::StaleTarget,
            format!(
                "{} changed while the edit ran, so writing it back would discard what was written there.",
                path.display()
            ),
        )
        .with_next("View the document again, then redo the edit on what it holds now.")
        .into())
    })
}

/// A temporary file beside `path`, synced and renamed over it once `ready` agrees; `shown` is the path that messages name.
/// With `permissions`, the temporary file is created readable by its owner only, and gets them before the rename. The
/// temporary file is removed on any failure, so a refusal or a failed rename leaves `path` as it was and nothing beside it.
fn write_through_temp(
    path: &Path,
    shown: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
    ready: impl FnOnce() -> Result<(), Failure>,
) -> Result<(), Failure> {
    let dir = parent(path);
    let tmp = temp_path(path);
    let private = permissions.is_some();
    let written = (|| -> std::io::Result<()> {
        // A new file, never one that is there: a leftover of a crashed process with the same id is removed first, and a
        // link planted under the temporary name is never followed.
        let _ = fs::remove_file(&tmp);
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        if private {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        #[cfg(not(unix))]
        let _ = private;
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(write_failed(shown, &e));
    }
    if let Err(refused) = ready() {
        let _ = fs::remove_file(&tmp);
        return Err(refused);
    }
    // The target was checked to be writable, so these permissions never make the temporary file read-only, which a
    // rename on Windows would refuse.
    let renamed = permissions
        .map_or(Ok(()), |p| fs::set_permissions(&tmp, p))
        .and_then(|()| fs::rename(&tmp, path));
    if let Err(e) = renamed {
        let _ = fs::remove_file(&tmp);
        return Err(write_failed(shown, &e));
    }
    sync_dir(&dir).map_err(|e| write_failed(shown, &e))
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

/// Whether two paths name the same file, after links and `..` are resolved.
pub fn same_file(a: &Path, b: &Path) -> bool {
    resolved(a) == resolved(b)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_atomic_refuses_a_file_that_changed_and_leaves_it_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("doc.docx");
        fs::write(&path, b"what the command read").unwrap();
        let read = sha256_hex(b"what the command read");
        // Someone else writes the file while the command runs.
        fs::write(&path, b"a newer version").unwrap();
        let Err(Failure::Core(error)) = replace_atomic(&path, b"the result", &read) else {
            panic!("a changed file is refused");
        };
        assert_eq!(error.code(), ErrorCode::StaleTarget);
        assert_eq!(fs::read(&path).unwrap(), b"a newer version");
        assert_eq!(
            list_dir(tmp.path()).unwrap(),
            ["doc.docx"],
            "no temporary file is left"
        );

        let now = sha256_hex(b"a newer version");
        replace_atomic(&path, b"the result", &now).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"the result");
        assert_eq!(list_dir(tmp.path()).unwrap(), ["doc.docx"]);
    }

    #[test]
    fn a_failed_rename_leaves_no_temporary_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("doc.docx");
        // Just before the rename the path becomes a folder that is not empty, which no rename can replace.
        let refused = write_through_temp(&path, &path, b"bytes", None, || {
            fs::create_dir(&path).unwrap();
            fs::write(path.join("inside"), b"x").unwrap();
            Ok(())
        });
        assert!(refused.is_err());
        assert_eq!(
            list_dir(tmp.path()).unwrap(),
            ["doc.docx"],
            "no temporary file is left"
        );
        assert!(path.is_dir());
    }

    #[test]
    fn a_read_only_file_is_not_replaced() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("doc.docx");
        fs::write(&path, b"old").unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions).unwrap();
        let refused = replace_atomic(&path, b"new", &sha256_hex(b"old"));
        assert!(refused.is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(list_dir(tmp.path()).unwrap(), ["doc.docx"]);
    }

    #[cfg(unix)]
    #[test]
    fn the_temporary_file_is_private_until_it_gets_the_targets_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("doc.docx");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let seen = std::cell::Cell::new(0);
        let target = fs::metadata(&path).unwrap().permissions();
        write_through_temp(&path, &path, b"new", Some(target), || {
            let mode = fs::metadata(temp_path(&path)).unwrap().permissions().mode();
            seen.set(mode & 0o777);
            Ok(())
        })
        .unwrap();
        assert_eq!(seen.get(), 0o600);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }
}
