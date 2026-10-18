//! The cache lock and the per-process work directory.
//!
//! Every git worktree on a machine shares one cache root, so fetching and set building run
//! under an exclusive lock on `<root>/.lock`. The warm path, with every blob and set already
//! present, takes no lock. While the lock is held no other fetcher exists, which is what lets
//! [`crate::corpus::sets::clean_leftovers`] delete every other `work` directory: they belong
//! to processes that were killed. A fetch keeps its git repositories in its own
//! `<root>/work/<pid>-<nanos>` directory, which [`Work`] removes when the fetch ends, also
//! when it fails.

use crate::repo::Res;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often a waiting process tries the lock again.
const POLL: Duration = Duration::from_millis(200);
/// How long a process waits for the lock before it gives up.
const GIVE_UP: Duration = Duration::from_secs(30 * 60);

/// The exclusive lock on the cache root. Dropping it releases the lock.
#[derive(Debug)]
pub struct Lock {
    _file: File,
}

/// Waits for and takes the exclusive lock on `<root>/.lock`, printing one line to stderr the
/// first time it has to wait.
pub fn acquire(root: &Path) -> Res<Lock> {
    acquire_with(root, POLL, GIVE_UP)
}

/// Like [`acquire`], with the polling interval and the time to give up after.
pub fn acquire_with(root: &Path, poll: Duration, give_up: Duration) -> Res<Lock> {
    fs::create_dir_all(root).map_err(|e| format!("cannot create {}: {e}", root.display()))?;
    let path = root.join(".lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("cannot open the lock file {}: {e}", path.display()))?;
    let started = Instant::now();
    let mut told = false;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(Lock { _file: file }),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(e)) => {
                return Err(format!("cannot lock {}: {e}", path.display()));
            }
        }
        if !told {
            eprintln!("waiting for another corpus fetch to finish");
            told = true;
        }
        if started.elapsed() >= give_up {
            return Err(format!(
                "gave up after {} seconds waiting for the lock file {}: another corpus fetch holds it; \
                 wait for it, or delete the file if no fetch is running",
                give_up.as_secs(),
                path.display()
            ));
        }
        std::thread::sleep(poll);
    }
}

/// A work directory owned by this process, removed when it is dropped.
#[derive(Debug)]
pub struct Work {
    dir: PathBuf,
}

impl Work {
    /// Creates `<root>/work/<pid>-<nanos>`. Call it while holding the [`Lock`], after
    /// [`crate::corpus::sets::clean_leftovers`].
    pub fn create(root: &Path) -> Res<Work> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = root
            .join("work")
            .join(format!("{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        Ok(Work { dir })
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
        if let Some(parent) = self.dir.parent() {
            // Succeeds only when no other work directory is left, which is the usual case.
            let _ = fs::remove_dir(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_taker_waits_and_then_gives_up_naming_the_lock_file() {
        let dir = tempfile::tempdir().expect("dir");
        let first = acquire(dir.path()).expect("first lock");
        let started = Instant::now();
        let err = acquire_with(
            dir.path(),
            Duration::from_millis(10),
            Duration::from_millis(150),
        )
        .expect_err("must give up");
        assert!(err.contains(".lock"), "{err}");
        assert!(started.elapsed() >= Duration::from_millis(150));
        drop(first);
        acquire(dir.path()).expect("free again");
    }

    #[test]
    fn a_work_directory_is_removed_when_dropped() {
        let dir = tempfile::tempdir().expect("dir");
        let work = Work::create(dir.path()).expect("work");
        let path = work.path().to_path_buf();
        fs::create_dir_all(path.join("demo")).expect("child");
        assert!(path.exists());
        drop(work);
        assert!(!path.exists());
    }
}
