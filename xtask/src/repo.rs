//! The repository on disk: which files exist and how to read them.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A result whose error is a message for the person running the command.
pub type Res<T> = Result<T, String>;

/// A repository root.
#[derive(Clone, Debug)]
pub struct Repo {
    root: PathBuf,
}

impl Repo {
    /// The repository this xtask was built in.
    pub fn discover() -> Res<Repo> {
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = manifest
            .parent()
            .ok_or_else(|| "xtask has no parent directory".to_string())?;
        Ok(Repo::at(root))
    }

    /// A repository rooted at `root`. Tests use this on temporary directories.
    pub fn at(root: impl Into<PathBuf>) -> Repo {
        Repo { root: root.into() }
    }

    /// The root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reads a file given relative to the root.
    pub fn read(&self, rel: &str) -> Res<String> {
        let path = self.root.join(rel);
        fs::read_to_string(&path).map_err(|e| format!("cannot read {rel}: {e}"))
    }

    /// Whether a path relative to the root exists.
    pub fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    /// Writes a file given relative to the root, creating parent directories.
    pub fn write(&self, rel: &str, content: &str) -> Res<()> {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
        }
        fs::write(&path, content).map_err(|e| format!("cannot write {rel}: {e}"))
    }

    /// Every file git tracks or would track (not ignored), relative to the root, in
    /// sorted order, with forward slashes. Outside a git repository it walks the
    /// directory instead, skipping `.git` and `target`.
    pub fn files(&self) -> Res<Vec<String>> {
        let listed = Command::new("git")
            .args([
                "ls-files",
                "--cached",
                "--others",
                "--exclude-standard",
                "-z",
            ])
            .current_dir(&self.root)
            .output();
        let mut files: Vec<String> = match listed {
            Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
                .split('\0')
                .filter(|s| !s.is_empty())
                .filter(|s| self.root.join(s).symlink_metadata().is_ok())
                .map(str::to_string)
                .collect(),
            _ => {
                let mut found = Vec::new();
                walk(&self.root, &self.root, &mut found)?;
                found
            }
        };
        files.sort();
        files.dedup();
        Ok(files)
    }

    /// Whether a path is a symbolic link.
    pub fn is_symlink(&self, rel: &str) -> bool {
        self.root
            .join(rel)
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
    }
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Res<()> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" || name == "target" {
            continue;
        }
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
pub mod testutil {
    //! Temporary repositories for tests.

    use super::Repo;
    use std::fs;
    use tempfile::TempDir;

    /// Creates a temporary repository containing `files` (path, content).
    pub fn repo(files: &[(&str, &str)]) -> (TempDir, Repo) {
        let dir = tempfile::tempdir().expect("create temp dir");
        for (path, content) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().expect("parent")).expect("create dirs");
            fs::write(full, content).expect("write file");
        }
        let repo = Repo::at(dir.path());
        (dir, repo)
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::repo;

    #[test]
    fn files_lists_a_plain_directory_sorted_and_skips_target() {
        let (_dir, repo) = repo(&[("b.md", ""), ("a/x.rs", ""), ("target/junk", "")]);
        assert_eq!(repo.files().expect("files"), ["a/x.rs", "b.md"]);
    }

    #[test]
    fn read_reports_the_missing_file() {
        let (_dir, repo) = repo(&[]);
        let err = repo.read("nope.md").expect_err("missing");
        assert!(err.contains("nope.md"), "{err}");
    }
}
