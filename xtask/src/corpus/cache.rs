//! The corpus cache on disk.
//!
//! ```text
//! <root>/blobs/<aa>/<sha256>                              verified bytes, read-only, content addressed
//! <root>/sets/<set>-<hash12>.<nanos>/<tier>/<sha12>-<name>   one generation of a set: hard links
//! <root>/sets/<set>-<hash12>.<nanos>/INDEX.tsv            one row per document
//! <root>/sets/<set>-<hash12>.<nanos>/COMPLETE             the manifest hash the set was built from
//! <root>/sets/<set>-<hash12>                              symlink to the newest generation
//! <root>/sets/<set>                                       symlink to the set of the last manifest
//! <root>/work/<pid>-<nanos>/<source>                      git work directories of one running fetch
//! <root>/.lock                                            exclusive lock held while fetching or building
//! ```
//!
//! `<root>` is `STETDOC_CORPUS_DIR` when set, otherwise `$XDG_CACHE_HOME/stetdoc/corpora`,
//! otherwise `$HOME/.cache/stetdoc/corpora`. Git worktrees on one machine share the root, so
//! every write happens under the lock (see [`crate::corpus::lock`]), a blob is written to a
//! temporary file and renamed into place, and a generation of a set is never changed after it
//! has its name, not even to repair a bad blob (see [`crate::corpus::sets`]). The warm path,
//! where every blob is present and read-only and the set is published, takes no lock. A reader
//! that resolves `sets/<set>` once keeps a directory that no fetch changes or removes while it
//! is in use. A `work` directory belongs to the process named in it and is removed when the
//! fetch ends, and the next fetch removes the ones killed runs left. Generations that are no
//! longer current are deleted after 24 hours unused.

use crate::corpus::manifest::{Doc, hex_sha256};
pub use crate::corpus::sets::set_name;
use crate::repo::Res;
use std::fs;
use std::path::{Path, PathBuf};

/// The environment variable that names the cache root.
pub const ENV: &str = "STETDOC_CORPUS_DIR";

/// Resolves the cache root from an explicit flag and the environment values.
pub fn resolve_dir(
    flag: Option<&str>,
    env: Option<&str>,
    xdg_cache: Option<&str>,
    home: Option<&str>,
) -> Res<PathBuf> {
    let non_empty = |v: Option<&str>| v.filter(|s| !s.is_empty()).map(PathBuf::from);
    if let Some(dir) = non_empty(flag).or_else(|| non_empty(env)) {
        return Ok(dir);
    }
    if let Some(dir) = non_empty(xdg_cache) {
        return Ok(dir.join("stetdoc").join("corpora"));
    }
    match non_empty(home) {
        Some(home) => Ok(home.join(".cache").join("stetdoc").join("corpora")),
        None => Err(format!(
            "no corpus cache location: set {ENV}, or HOME, or pass --dir"
        )),
    }
}

/// The cache root for this process.
pub fn dir_from_environment(flag: Option<&str>) -> Res<PathBuf> {
    resolve_dir(
        flag,
        std::env::var(ENV).ok().as_deref(),
        std::env::var("XDG_CACHE_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Where a document's verified bytes live.
pub fn blob_path(root: &Path, sha256: &str) -> PathBuf {
    root.join("blobs").join(&sha256[..2]).join(sha256)
}

/// Whether the cache holds a good copy of `doc`. The size is always checked; the hash
/// only when `rehash` is set.
pub fn have(root: &Path, doc: &Doc, rehash: bool) -> bool {
    let path = blob_path(root, &doc.sha256);
    let Ok(meta) = fs::metadata(&path) else {
        return false;
    };
    if meta.len() != doc.size {
        return false;
    }
    !rehash || fs::read(&path).is_ok_and(|b| hex_sha256(&b) == doc.sha256)
}

/// Whether the blob of `doc` is read-only. A blob stored by an earlier version of this tool
/// is not, and a fetch fixes that.
pub fn is_protected(root: &Path, doc: &Doc) -> bool {
    fs::metadata(blob_path(root, &doc.sha256)).is_ok_and(|m| m.permissions().readonly())
}

/// Makes the blob of `doc` read-only, so a stray write cannot corrupt it.
pub fn protect(root: &Path, doc: &Doc) -> Res<()> {
    let path = blob_path(root, &doc.sha256);
    let mut permissions = fs::metadata(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&path, permissions)
        .map_err(|e| format!("cannot protect {}: {e}", path.display()))
}

/// Stores verified bytes atomically, so an interrupted fetch never leaves a bad blob. The
/// blob is read-only, so a stray write cannot corrupt it.
pub fn store(root: &Path, doc: &Doc, bytes: &[u8]) -> Res<()> {
    let path = blob_path(root, &doc.sha256);
    let dir = path.parent().ok_or("blob path has no parent")?;
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let tmp = dir.join(format!(".tmp-{}-{}", doc.sha256, std::process::id()));
    // A killed run can leave a read-only temporary file that a plain write would refuse.
    let _ = fs::remove_file(&tmp);
    fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    let mut permissions = fs::metadata(&tmp)
        .map_err(|e| format!("cannot read {}: {e}", tmp.display()))?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&tmp, permissions)
        .map_err(|e| format!("cannot protect {}: {e}", tmp.display()))?;
    fs::rename(&tmp, &path).map_err(|e| format!("cannot move {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_location_prefers_the_flag_then_the_variable_then_xdg_then_home() {
        let r = |f, e, x, h| resolve_dir(f, e, x, h).expect("resolves");
        assert_eq!(
            r(Some("/f"), Some("/e"), Some("/x"), Some("/h")),
            PathBuf::from("/f")
        );
        assert_eq!(
            r(None, Some("/e"), Some("/x"), Some("/h")),
            PathBuf::from("/e")
        );
        assert_eq!(
            r(None, None, Some("/x"), Some("/h")),
            PathBuf::from("/x/stetdoc/corpora")
        );
        assert_eq!(
            r(None, Some(""), None, Some("/h")),
            PathBuf::from("/h/.cache/stetdoc/corpora")
        );
        assert!(resolve_dir(None, None, None, None).is_err());
    }

    fn doc(bytes: &[u8]) -> Doc {
        Doc {
            sha256: hex_sha256(bytes),
            size: bytes.len() as u64,
            tier: "lab".into(),
            expect: "ok".into(),
            stage: "-".into(),
            part: "-".into(),
            reason: "-".into(),
            spdx: "MIT".into(),
            producer: "p".into(),
            word_fp: "n".into(),
            path: "d/a b.docx".into(),
            source: "s".into(),
            origin: (String::new(), 0),
        }
    }

    #[test]
    fn a_stored_blob_is_found_and_a_corrupt_one_is_not() {
        let dir = tempfile::tempdir().expect("dir");
        let d = doc(b"hello");
        assert!(!have(dir.path(), &d, false));
        store(dir.path(), &d, b"hello").expect("store");
        assert!(have(dir.path(), &d, true));
        let blob = blob_path(dir.path(), &d.sha256);
        fs::remove_file(&blob).expect("remove");
        fs::write(&blob, b"HELLO").expect("corrupt");
        assert!(
            have(dir.path(), &d, false),
            "same size passes the cheap check"
        );
        assert!(!have(dir.path(), &d, true), "the rehash catches it");
        fs::remove_file(&blob).expect("remove");
        fs::write(&blob, b"hi").expect("truncate");
        assert!(!have(dir.path(), &d, false));
    }

    #[test]
    fn a_stored_blob_is_read_only_and_a_leftover_temporary_file_does_not_block_a_store() {
        let dir = tempfile::tempdir().expect("dir");
        let d = doc(b"hello");
        let blob = blob_path(dir.path(), &d.sha256);
        let shard = blob.parent().expect("shard").to_path_buf();
        fs::create_dir_all(&shard).expect("shard");
        let stale = shard.join(format!(".tmp-{}-{}", d.sha256, std::process::id()));
        fs::write(&stale, b"half").expect("stale");
        let mut p = fs::metadata(&stale).expect("meta").permissions();
        p.set_readonly(true);
        fs::set_permissions(&stale, p).expect("protect");
        store(dir.path(), &d, b"hello").expect("store");
        assert!(
            fs::metadata(&blob).expect("meta").permissions().readonly(),
            "the blob is read-only"
        );
        assert!(fs::OpenOptions::new().append(true).open(&blob).is_err());
        assert_eq!(fs::read(&blob).expect("read"), b"hello");
    }
}
