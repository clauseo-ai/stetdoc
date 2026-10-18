//! A working copy on disk: reading it into a `WorkingCopy`, the advisory lock, executing the core's `WritePlan` one step at
//! a time, and recovering an interrupted write (design `cli-and-skill.md` 3, core design `document-and-ops.md` 5.4).
//!
//! The order of the writes is the core's, because the order is what makes a crash harmless. This file only carries the
//! steps out: every write is a temporary file, an fsync and an atomic rename, the directory is synced after the commit
//! and at the end, and the whole command holds an exclusive lock on `.stet/lock`.

use super::{is_temp_name, parent, sync_dir, write_atomic, write_failed};
use crate::output::Failure;
use sha2::{Digest, Sha256};
use std::fs::{self, File, TryLockError};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};
use stetdoc_core::{
    Complement, Document, ErrorCode, Limits, OpenOptions, StetError, WorkingCopy, WriteOptions,
    WriteStep,
};

/// The complement's folder inside a working copy.
pub const STET: &str = ".stet";
/// The CLI's lock file in `.stet/`. It is never read as an entry.
const LOCK: &str = "lock";
/// The CLI's record in `.stet/` of the SHA-256 of every .docx this working copy packed, one per line, which lets `pack`
/// overwrite its own output and refuse anything else. It is never read as an entry.
const PACKED: &str = "packed";
/// The complement's copy of the document the working copy came from (`document-and-ops.md` 5.1).
const ORIGINAL: &str = "original.docx";
/// The largest record of packed outputs that is read.
const MAX_PACKED: u64 = 1 << 20;
/// The interruption marker in `.stet/`.
const MARKER: &str = "writing";
/// The manifest in `.stet/`.
const MANIFEST: &str = "manifest";
/// How long a command waits for another process to finish with the working copy.
const LOCK_WAIT: Duration = Duration::from_secs(30);

/// A locked working copy. The lock is released when this is dropped, or by the OS when the process dies.
pub struct Workdir {
    dir: PathBuf,
    _lock: File,
}

impl Workdir {
    /// Locks an existing working copy and finishes or undoes an interrupted write. Returns the notes the agent must see:
    /// the warnings of a recovery.
    pub fn open(dir: &Path) -> Result<(Workdir, Vec<String>), Failure> {
        let stet = dir.join(STET);
        if stet
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(StetError::new(
                ErrorCode::CorruptComplement,
                format!("{} is a symbolic link, and a working copy's complement is a folder of its own.", stet.display()),
            )
            .with_next("Unpack the .docx again into a new folder.")
            .into());
        }
        if !stet.join(MANIFEST).is_file() && !stet.join(MARKER).is_file() {
            return Err(StetError::new(
                ErrorCode::NotFound,
                format!(
                    "{} is a directory, and not a working copy: it has no .stet/manifest.",
                    dir.display()
                ),
            )
            .with_next("Name the folder that stetdoc unpack wrote, or unpack the .docx first.")
            .into());
        }
        let workdir = Workdir {
            dir: dir.to_path_buf(),
            _lock: lock(&stet)?,
        };
        let notes = workdir.recover()?;
        Ok((workdir, notes))
    }

    /// Creates the directory of a new working copy and locks it. An existing directory must be empty. Emptiness is checked
    /// again once the lock is held, so two unpacks into one folder cannot both write it.
    pub fn create(dir: &Path) -> Result<Workdir, Failure> {
        if dir.exists() && !empty_but_lock(dir) {
            return Err(occupied(dir));
        }
        let stet = dir.join(STET);
        fs::create_dir_all(&stet).map_err(|e| write_failed(&stet, &e))?;
        let lock = lock(&stet)?;
        if !empty_but_lock(dir) {
            return Err(occupied(dir));
        }
        Ok(Workdir {
            dir: dir.to_path_buf(),
            _lock: lock,
        })
    }

    /// Removes what a failed unpack wrote: everything in the directory, which was empty when `create` locked it, and the
    /// directory itself when it did not exist before.
    pub fn discard(self, existed: bool) {
        let dir = self.dir.clone();
        drop(self);
        if existed {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let _ = if path.is_dir() {
                        fs::remove_dir_all(&path)
                    } else {
                        fs::remove_file(&path)
                    };
                }
            }
        } else {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    /// The working copy's directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Reads the working copy: the readable files and the complement's entries, without the marker.
    pub fn read(&self) -> Result<WorkingCopy, Failure> {
        self.read_with(false)
    }

    /// Reads and resumes the working copy.
    pub fn resume(&self) -> Result<(Document, WorkingCopy), Failure> {
        let copy = self.read()?;
        let doc = Document::resume(&copy, &OpenOptions::default())?;
        Ok((doc, copy))
    }

    /// Writes `next` over the working copy that is on disk now (`disk`, `None` for a new one), by the core's plan.
    pub fn write(
        &self,
        next: &WorkingCopy,
        disk: Option<&WorkingCopy>,
        options: &WriteOptions,
    ) -> Result<(), Failure> {
        let plan = next.write_plan(disk, options)?;
        self.execute(&plan.steps, usize::MAX)
    }

    /// Executes at most `limit` steps, in order. Only the crash tests pass a limit.
    pub(crate) fn execute(&self, steps: &[WriteStep], limit: usize) -> Result<(), Failure> {
        for step in steps.iter().take(limit) {
            self.step(step)?;
        }
        if limit >= steps.len() {
            sync_dir(&self.dir.join(STET)).map_err(|e| write_failed(&self.dir, &e))?;
            sync_dir(&self.dir).map_err(|e| write_failed(&self.dir, &e))?;
        }
        Ok(())
    }

    fn step(&self, step: &WriteStep) -> Result<(), Failure> {
        match step {
            WriteStep::Marker { path, bytes } | WriteStep::Entry { path, bytes } => {
                write_atomic(&self.path(path)?, bytes)
            }
            WriteStep::Commit { path, bytes } => {
                let target = self.path(path)?;
                write_atomic(&target, bytes)?;
                // The rename of the manifest is the commit point, so it is made durable before any readable file changes.
                sync_dir(&parent(&target)).map_err(|e| write_failed(&target, &e))
            }
            WriteStep::File { path, text } => write_atomic(&self.path(path)?, text.as_bytes()),
            WriteStep::Remove { path } => {
                let target = self.path(path)?;
                match fs::remove_file(&target) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(write_failed(&target, &e)),
                }
            }
        }
    }

    /// The path of a step, which the core gives relative to the working copy. Anything that would leave the directory is a
    /// bug in stetdoc, and is refused before it touches the disk.
    fn path(&self, relative: &str) -> Result<PathBuf, Failure> {
        let path = Path::new(relative);
        let inside =
            !relative.is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)));
        if !inside {
            return Err(StetError::new(
                ErrorCode::Internal,
                format!("internal: a write step names {relative}, which is not a path inside the working copy."),
            )
            .into());
        }
        Ok(self.dir.join(path))
    }

    /// Finishes or undoes an interrupted write, when `.stet/writing` exists. Returns the warnings to print.
    fn recover(&self) -> Result<Vec<String>, Failure> {
        let marker_path = self.dir.join(STET).join(MARKER);
        if !marker_path.is_file() {
            return Ok(Vec::new());
        }
        let disk = self.read_with(true)?;
        let marker = disk
            .complement
            .entries
            .iter()
            .find(|(name, _)| name == MARKER)
            .map(|(_, bytes)| bytes.clone())
            .unwrap_or_default();
        let recovery = WorkingCopy::recover(&disk, &marker)?;
        let mut notes: Vec<String> = recovery
            .warnings
            .iter()
            .map(|w| format!("warning: {w}"))
            .collect();
        self.execute(&recovery.steps, usize::MAX)?;
        if !recovery.rerender.is_empty() {
            // The committed head renders the files again. `resume` refuses a directory with its marker, which `read` leaves out.
            let (doc, _) = self.resume()?;
            let files = doc.files()?;
            for name in &recovery.rerender {
                let target = self.path(name)?;
                match files.files.iter().find(|(n, _)| n == name) {
                    Some((_, text)) => write_atomic(&target, text.as_bytes())?,
                    None => self.step(&WriteStep::Remove { path: name.clone() })?,
                }
            }
        }
        self.execute(std::slice::from_ref(&recovery.finish), usize::MAX)?;
        notes.push(format!(
            "note: {} had an interrupted write, and it was recovered.",
            self.dir.display()
        ));
        Ok(notes)
    }

    /// The SHA-256 digests of what this working copy may overwrite when it packs: the document it came from, and every
    /// .docx it packed before.
    pub fn own_digests(&self, copy: &WorkingCopy) -> Result<Vec<String>, Failure> {
        let mut digests: Vec<String> = copy
            .complement
            .entries
            .iter()
            .filter(|(name, _)| name == ORIGINAL)
            .map(|(_, bytes)| super::sha256_hex(bytes))
            .collect();
        let record = self.dir.join(STET).join(PACKED);
        refuse_link(&record)?;
        if let Some(text) = super::read_text_if_any(&record, MAX_PACKED)? {
            digests.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|l| l.len() == 64 && l.bytes().all(|b| b.is_ascii_hexdigit()))
                    .map(str::to_ascii_lowercase),
            );
        }
        Ok(digests)
    }

    /// Records the SHA-256 of a .docx that this working copy is about to pack. It is written before the .docx, so a crash in
    /// between leaves a record of a file never written, which is harmless, and never a packed file without a record.
    pub fn record_packed(&self, digest: &str) -> Result<(), Failure> {
        let record = self.dir.join(STET).join(PACKED);
        refuse_link(&record)?;
        let mut text = super::read_text_if_any(&record, MAX_PACKED)?.unwrap_or_default();
        if text.lines().any(|line| line.trim() == digest) {
            return Ok(());
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(digest);
        text.push('\n');
        write_atomic(&record, text.as_bytes())
    }

    /// Saves every readable file that is not UTF-8 aside, as `<name>.<8 hex digits of its SHA-256>.stale`, and removes it, so
    /// that a refresh can render it again. Returns the warnings that name the saved files. Only `unpack --refresh` calls it.
    pub fn set_aside_undecodable(&self) -> Result<Vec<String>, Failure> {
        let mut notes = Vec::new();
        let extensions = readable_extensions();
        for (name, path) in regular_files(&self.dir)? {
            if !has_extension(&name, &extensions) {
                continue;
            }
            let bytes = super::read_file(&path, Limits::default().max_entry_bytes)?;
            if std::str::from_utf8(&bytes).is_ok() {
                continue;
            }
            let digest = Sha256::digest(&bytes);
            let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
            let aside = format!("{name}.{hex}.stale");
            write_atomic(&self.dir.join(&aside), &bytes)?;
            fs::remove_file(&path).map_err(|e| write_failed(&path, &e))?;
            notes.push(format!(
                "warning: {name} was not UTF-8 text. It is saved as {aside}, and {name} is rendered again."
            ));
        }
        Ok(notes)
    }

    fn read_with(&self, marker: bool) -> Result<WorkingCopy, Failure> {
        let limits = Limits::default();
        let extensions = readable_extensions();
        let mut files = Vec::new();
        for (name, path) in regular_files(&self.dir)? {
            if !has_extension(&name, &extensions) {
                continue;
            }
            let bytes = super::read_file(&path, limits.max_entry_bytes)?;
            let text = String::from_utf8(bytes).map_err(|_| {
                Failure::Core(
                    StetError::new(
                        ErrorCode::InvalidPayload,
                        format!("{name} is not UTF-8 text."),
                    )
                    .with_next(
                        "Save the file as UTF-8, or restore it with stetdoc unpack DIR --refresh.",
                    ),
                )
            })?;
            files.push((name, text));
        }
        let mut entries = Vec::new();
        for (name, path) in regular_files(&self.dir.join(STET))? {
            if name == LOCK || name == PACKED || is_temp_name(&name) || (!marker && name == MARKER)
            {
                continue;
            }
            entries.push((name, super::read_file(&path, limits.max_archive_bytes)?));
        }
        Ok(WorkingCopy {
            files,
            complement: Complement { entries },
        })
    }
}

/// The extensions of readable files, from the formats the core lists.
fn readable_extensions() -> Vec<String> {
    stetdoc_core::capabilities()
        .formats
        .into_iter()
        .map(|f| f.extension)
        .collect()
}

/// Whether a file name has one of the readable extensions.
fn has_extension(name: &str, extensions: &[String]) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| extensions.iter().any(|x| x == e))
}

/// Whether a directory is empty, apart from a `.stet/` that holds only the lock file: what `create` leaves before it writes.
fn empty_but_lock(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().all(|entry| {
        let real_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        entry.file_name() == STET
            && real_dir
            && fs::read_dir(entry.path())
                .is_ok_and(|inner| inner.flatten().all(|e| e.file_name() == LOCK))
    })
}

/// The refusal of a folder that already has something in it.
fn occupied(dir: &Path) -> Failure {
    let next = if dir.join(STET).join(MANIFEST).is_file() {
        "It is a working copy already: keep using it, choose another folder with -o, or run stetdoc unpack DIR --refresh to render it again."
    } else {
        "Choose another folder with -o, or empty this one."
    };
    Failure::usage(
        format!(
            "{} already exists and is not empty, and stetdoc unpack writes a new working copy.",
            dir.display()
        ),
        next,
    )
}

/// Refuses a file of the complement that is a symbolic link, so that opening it never reaches a file outside the working
/// copy. The check comes just before the open, which leaves a window only for a process that can already write `.stet/`.
fn refuse_link(path: &Path) -> Result<(), Failure> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(StetError::new(
            ErrorCode::CorruptComplement,
            format!(
                "{} is a symbolic link, and the files of a working copy's complement are its own.",
                path.display()
            ),
        )
        .with_next("Unpack the .docx again into a new folder.")
        .into());
    }
    Ok(())
}

/// The regular files of a directory, by name, sorted. Symlinks are not followed, so a link cannot pull a file from outside
/// the working copy into it.
fn regular_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, Failure> {
    let entries = fs::read_dir(dir).map_err(|e| {
        Failure::Core(
            StetError::new(
                ErrorCode::NotFound,
                format!("Could not read the folder {}: {e}.", dir.display()),
            )
            .with_next("Check the path of the working copy."),
        )
    })?;
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        if let Some(name) = entry.file_name().to_str() {
            found.push((name.to_string(), entry.path()));
        }
    }
    found.sort();
    Ok(found)
}

/// Takes the exclusive lock on `.stet/lock`, waiting up to `LOCK_WAIT` for another process.
fn lock(stet: &Path) -> Result<File, Failure> {
    let path = stet.join(LOCK);
    refuse_link(&path)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|e| write_failed(&path, &e))?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(TryLockError::WouldBlock) if start.elapsed() < LOCK_WAIT => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(Failure::io(
                    format!(
                        "Another stetdoc process is using {}.",
                        parent(stet).display()
                    ),
                    "Wait for it to finish, then run the command again.",
                ));
            }
            Err(TryLockError::Error(e)) => return Err(write_failed(&path, &e)),
        }
    }
}

#[cfg(test)]
#[path = "workdir_tests.rs"]
mod tests;
