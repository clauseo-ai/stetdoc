//! Sets: directories of hard links to verified blobs, built once and never modified.
//!
//! A set is built in `<root>/sets/.tmp-<set>-<pid>`, which gets its `INDEX.tsv` and `COMPLETE`
//! marker, and is then renamed to a generation directory `<root>/sets/<set>-<hash12>.<nanos>`.
//! A directory with that name is therefore always complete and is never touched again, not even
//! to repair a damaged blob: a repair builds a new generation. Two symlinks lead to the newest
//! one, and each is replaced atomically, because a directory cannot take the place of another
//! that is not empty:
//!
//! - `<root>/sets/<set>-<hash12>` is the name of the set for one manifest, and the one the tests
//!   read (`STETDOC_CORPUS_SET`). It points at the newest generation.
//! - `<root>/sets/<set>` is the convenience name and points at the stable name of the manifest
//!   that was fetched last.
//!
//! A reader that opens a set resolves the links once and works inside the generation they name,
//! which no fetch changes or removes while it is in use. A reader that keeps opening paths
//! through the links moves to a newer generation between two reads, which holds the same files
//! by name. Neither can meet a missing file. The one exception is the first repair of a set made
//! by the previous layout, which stored `<set>-<hash12>` as a real directory: it is renamed to a
//! generation name and the link takes its place, which leaves the name absent for an instant.
//!
//! A generation that is no longer reached by the link of its manifest, and a set of another
//! manifest, is deleted only when its modification time is older than [`KEEP`], because a reader
//! may still be using it. Publishing and the warm path refresh the time of the generation in
//! use, so it measures the last use. Everything here that writes runs under the cache lock.
//!
//! Directory symlinks need no privilege on Linux and macOS. On Windows creating one needs
//! Developer Mode or an elevated shell, and a fetch there says so when it fails. Windows also
//! cannot rename over a link, so it removes the old link first, and a reader that resolves the
//! name in that instant fails once and succeeds on a retry. CI does not run on Windows yet; a
//! portable pointer file would replace the links if it ever does.

use crate::corpus::cache::blob_path;
use crate::corpus::manifest::Doc;
use crate::repo::Res;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a superseded set stays on disk before a fetch deletes it.
pub const KEEP: Duration = Duration::from_secs(24 * 60 * 60);

/// The first line of `INDEX.tsv`. The columns up to `spdx` keep their order for readers that
/// were written against them, and `stage`, `reason` and `part` follow.
pub const INDEX_HEADER: &str =
    "id\ttier\texpect\tsha256\tsize\tpath\tproducer\tword_fp\tspdx\tstage\treason\tpart\n";

/// The id used in `INDEX.tsv`: the source and the path inside it.
pub fn doc_id(doc: &Doc) -> String {
    format!("{}:{}", doc.source, doc.path)
}

/// The stable name of `set` for a manifest: the set name and the first twelve digits of the
/// manifest hash. It is a symlink to the newest generation of the set.
pub fn set_name(set: &str, manifest_hash: &str) -> String {
    let short = manifest_hash.get(..12).unwrap_or(manifest_hash);
    format!("{set}-{short}")
}

/// The convenience name of a set, a symlink to its current immutable directory.
pub fn set_dir(root: &Path, set: &str) -> PathBuf {
    root.join("sets").join(set)
}

/// The stable path of `set` for a manifest hash. It resolves to the newest generation.
pub fn immutable_dir(root: &Path, set: &str, manifest_hash: &str) -> PathBuf {
    root.join("sets").join(set_name(set, manifest_hash))
}

/// Whether `set` exists for this manifest hash and was built from it.
pub fn set_complete(root: &Path, set: &str, manifest_hash: &str) -> bool {
    fs::read_to_string(immutable_dir(root, set, manifest_hash).join("COMPLETE"))
        .is_ok_and(|text| text.lines().next() == Some(manifest_hash))
}

/// Whether the set is complete and `sets/<set>` points at it.
pub fn is_published(root: &Path, set: &str, manifest_hash: &str) -> bool {
    set_complete(root, set, manifest_hash)
        && fs::read_link(set_dir(root, set))
            .is_ok_and(|target| target == Path::new(&set_name(set, manifest_hash)))
}

/// Removes a file, a symlink or a directory tree, and says nothing when it is not there.
fn remove_any(path: &Path) -> Res<()> {
    let Ok(meta) = path.symlink_metadata() else {
        return Ok(());
    };
    let done = if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match done {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("cannot remove {}: {e}", path.display()))
        }
        _ => Ok(()),
    }
}

/// The time as a number that makes a generation name unique.
fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Builds a new generation of a set from blobs that are all present and points the stable name
/// at it. An existing complete set is kept unless `replace` is set, which a repair of a bad blob
/// needs because the links of the old generation still name the old file. The old generation is
/// left where it is, for readers that use it, and [`prune`] removes it when it is old.
pub fn build_set(
    root: &Path,
    set: &str,
    docs: &[&Doc],
    manifest_hash: &str,
    replace: bool,
) -> Res<()> {
    if !replace && set_complete(root, set, manifest_hash) {
        return Ok(());
    }
    let sets = root.join("sets");
    fs::create_dir_all(&sets).map_err(|e| format!("cannot create {}: {e}", sets.display()))?;
    let pid = std::process::id();
    let tmp = sets.join(format!(".tmp-{set}-{pid}"));
    remove_any(&tmp)?;
    fs::create_dir(&tmp).map_err(|e| format!("cannot create {}: {e}", tmp.display()))?;
    if let Err(e) = fill(root, &tmp, docs, manifest_hash) {
        let _ = remove_any(&tmp);
        return Err(e);
    }
    let stable_name = set_name(set, manifest_hash);
    let generation = format!("{stable_name}.{}", nanos());
    fs::rename(&tmp, sets.join(&generation))
        .map_err(|e| format!("cannot move {}: {e}", tmp.display()))?;
    let stable = sets.join(&stable_name);
    if stable
        .symlink_metadata()
        .is_ok_and(|meta| !meta.file_type().is_symlink())
    {
        // The previous layout kept the set itself under this name. It becomes a generation.
        let old = sets.join(format!("{stable_name}.{}", nanos()));
        let link = sets.join(format!(".tmp-link-{stable_name}-{pid}"));
        remove_any(&link)?;
        symlink_dir(&generation, &link)
            .map_err(|e| format!("cannot link {}: {e}", stable.display()))?;
        fs::rename(&stable, &old)
            .map_err(|e| format!("cannot move {} aside: {e}", stable.display()))?;
        return fs::rename(&link, &stable)
            .map_err(|e| format!("cannot publish {}: {e}", stable.display()));
    }
    replace_link(
        &generation,
        &stable,
        &sets.join(format!(".tmp-link-{stable_name}-{pid}")),
    )
}

/// Writes the links, `INDEX.tsv` and `COMPLETE` of a set into `dir`.
fn fill(root: &Path, dir: &Path, docs: &[&Doc], manifest_hash: &str) -> Res<()> {
    let mut index = String::from(INDEX_HEADER);
    for doc in docs {
        let rel = format!("{}/{}", doc.tier, doc.file_name());
        let target = dir.join(&rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        let blob = blob_path(root, &doc.sha256);
        if fs::hard_link(&blob, &target).is_err() {
            fs::copy(&blob, &target)
                .map_err(|e| format!("cannot place {}: {e}", target.display()))?;
        }
        index.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            doc_id(doc),
            doc.tier,
            doc.expect,
            doc.sha256,
            doc.size,
            rel,
            doc.producer,
            doc.word_fp,
            doc.spdx,
            doc.stage,
            doc.reason,
            doc.part
        ));
    }
    fs::write(dir.join("INDEX.tsv"), index).map_err(|e| format!("cannot write INDEX.tsv: {e}"))?;
    fs::write(
        dir.join("COMPLETE"),
        format!("{manifest_hash}\n{}\n", docs.len()),
    )
    .map_err(|e| format!("cannot write COMPLETE: {e}"))
}

/// Creates a directory symlink.
#[cfg(unix)]
fn symlink_dir(target: &str, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

/// Creates a directory symlink. Windows needs Developer Mode or an elevated shell for this, so
/// the error says so.
#[cfg(windows)]
fn symlink_dir(target: &str, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link).map_err(|e| {
        std::io::Error::new(
            e.kind(),
            format!("{e}; creating a directory symlink on Windows needs Developer Mode or an elevated shell"),
        )
    })
}

/// Creates a directory symlink.
#[cfg(not(any(unix, windows)))]
fn symlink_dir(_target: &str, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("symlinks are not supported here"))
}

/// Makes `link` a symlink to `target` (a name next to it), replacing an older link atomically.
/// The new link is made under `tmp` first and renamed over `link`.
fn replace_link(target: &str, link: &Path, tmp: &Path) -> Res<()> {
    remove_any(tmp)?;
    symlink_dir(target, tmp)
        .map_err(|e| format!("cannot link {} to {target}: {e}", link.display()))?;
    // Windows cannot rename over an existing link, so it is removed first there.
    #[cfg(not(unix))]
    remove_any(link)?;
    fs::rename(tmp, link).map_err(|e| format!("cannot publish {}: {e}", link.display()))
}

/// Makes `sets/<set>` a symlink to the stable name of the manifest, replacing an older link
/// atomically, or an old directory of the first layout.
pub fn publish(root: &Path, set: &str, manifest_hash: &str) -> Res<()> {
    let name = set_name(set, manifest_hash);
    let link = set_dir(root, set);
    let current = immutable_dir(root, set, manifest_hash);
    if is_published(root, set, manifest_hash) {
        touch(&current);
        return Ok(());
    }
    if link
        .symlink_metadata()
        .is_ok_and(|meta| !meta.file_type().is_symlink())
    {
        remove_any(&link)?;
    }
    let tmp = root
        .join("sets")
        .join(format!(".tmp-link-{set}-{}", std::process::id()));
    replace_link(&name, &link, &tmp)?;
    touch(&current);
    Ok(())
}

/// Sets the modification time of a directory to now, so it measures the last use. A failure
/// is ignored: the time only protects a set from early deletion.
pub fn touch(dir: &Path) {
    if let Ok(file) = fs::File::open(dir) {
        let _ = file.set_modified(SystemTime::now());
    }
}

/// Whether `text` is `n` lower-case hex digits.
fn is_hex(text: &str, n: usize) -> bool {
    text.len() == n && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The age of what `path` leads to, or `None` when it leads nowhere.
fn age_of(path: &Path) -> Option<Duration> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    SystemTime::now().duration_since(modified).ok()
}

/// Deletes what is no longer in use: the superseded generations of `set` for this manifest, and
/// the stable names and generations of other manifests, when their modification time is older
/// than `keep` or their link leads nowhere. Returns how many entries it deleted.
pub fn prune(root: &Path, set: &str, manifest_hash: &str, keep: Duration) -> usize {
    let current = set_name(set, manifest_hash);
    let live = fs::read_link(root.join("sets").join(&current))
        .ok()
        .and_then(|target| target.to_str().map(str::to_string));
    let prefix = format!("{set}-");
    let Ok(entries) = fs::read_dir(root.join("sets")) else {
        return 0;
    };
    let mut deleted = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // `<set>-<hash12>` is a stable name (or a directory of the first layout) and
        // `<set>-<hash12>.<nanos>` is a generation.
        let Some(rest) = name.strip_prefix(&prefix) else {
            continue;
        };
        let (digits, generation) = match rest.split_once('.') {
            Some((digits, nanos)) => (digits, Some(nanos)),
            None => (rest, None),
        };
        let shaped = is_hex(digits, 12)
            && generation.is_none_or(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !shaped || name == current || live.as_deref() == Some(name.as_str()) {
            continue;
        }
        let path = entry.path();
        let unused = match age_of(&path) {
            Some(age) => age > keep,
            None => true,
        };
        if unused && remove_any(&path).is_ok() {
            deleted += 1;
        }
    }
    deleted
}

/// Removes what killed or older processes left behind. The caller holds the exclusive lock,
/// so no live fetcher exists: every directory in `work`, the shared `git` directory of the
/// previous layout, every `.tmp-` entry in `sets` and every `.tmp-` file in `blobs` is
/// garbage. Failures are ignored, because leftovers cost space and nothing else.
pub fn clean_leftovers(root: &Path) {
    let _ = remove_any(&root.join("git"));
    let each = |dir: PathBuf, only_temp: bool, then: &dyn Fn(&Path)| {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let temp = entry.file_name().to_string_lossy().starts_with(".tmp-");
                if temp || !only_temp {
                    then(&entry.path());
                }
            }
        }
    };
    each(root.join("work"), false, &|p| {
        let _ = remove_any(p);
    });
    each(root.join("sets"), true, &|p| {
        let _ = remove_any(p);
    });
    each(root.join("blobs"), false, &|shard| {
        each(shard.to_path_buf(), true, &|p| {
            let _ = remove_any(p);
        });
    });
}
