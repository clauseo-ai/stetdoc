//! `--root`: every path argument of a tool call is resolved against one folder and must stay inside it, for a server that
//! hosts documents for someone else.
//!
//! A path is joined to the root when it is relative, normalized without touching the disk, and refused when a `..` takes
//! it out. Then the deepest part of it that exists is canonicalized, which follows every symbolic link, and refused when
//! that lands outside the root. What is returned is the canonical existing part joined with the rest, so the command never
//! sees a link. Canonical paths come from `dunce`, which leaves out the Windows `\\?\` prefix where it can, so a relative
//! path with forward slashes joins them as it does on Unix.
//!
//! A link created between this check and the command's use of the path is not caught, and neither is one planted inside a
//! working copy. The root is meant for a folder only the server writes.

use crate::output::Failure;
use std::path::{Component, Path, PathBuf};

/// The folder paths are confined to, canonical.
pub struct Root {
    dir: PathBuf,
}

impl Root {
    /// The root, which must be an existing folder.
    pub fn new(dir: &Path) -> Result<Root, Failure> {
        let canonical = dunce::canonicalize(dir).map_err(|e| {
            Failure::usage(
                format!("--root {}: {e}.", dir.display()),
                "Name an existing folder with --root.",
            )
        })?;
        if !canonical.is_dir() {
            return Err(Failure::usage(
                format!("--root {} is not a folder.", dir.display()),
                "Name an existing folder with --root.",
            ));
        }
        Ok(Root { dir: canonical })
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A path argument, resolved against the root and confined to it.
    pub fn resolve(&self, value: &str) -> Result<PathBuf, Failure> {
        if value.is_empty() {
            return Err(Failure::usage(
                "A path argument is empty.",
                "Give a path inside the root.",
            ));
        }
        let path = Path::new(value);
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.dir.join(path)
        };
        self.confine(&joined, value)
    }

    /// A path the server computed, such as a command's default output, confined to the root.
    pub fn check(&self, path: &Path) -> Result<PathBuf, Failure> {
        let shown = path.display().to_string();
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.dir.join(path)
        };
        self.confine(&joined, &shown)
    }

    fn confine(&self, joined: &Path, shown: &str) -> Result<PathBuf, Failure> {
        let normal = normalize(joined).ok_or_else(|| self.outside(shown))?;
        if !normal.starts_with(&self.dir) {
            return Err(self.outside(shown));
        }
        // The deepest ancestor that exists (as a link or anything else), canonicalized.
        let mut existing = normal.as_path();
        let mut rest = Vec::new();
        while existing.symlink_metadata().is_err() {
            let (Some(parent), Some(name)) = (existing.parent(), existing.file_name()) else {
                return Err(self.outside(shown));
            };
            rest.push(name.to_os_string());
            existing = parent;
        }
        let canonical = dunce::canonicalize(existing).map_err(|_| {
            Failure::usage(
                format!("{shown} goes through a symbolic link to nothing."),
                "Give a path inside the root that does not go through a broken link.",
            )
        })?;
        if !canonical.starts_with(&self.dir) {
            return Err(self.outside(shown));
        }
        let mut out = canonical;
        for name in rest.iter().rev() {
            out.push(name);
        }
        Ok(out)
    }

    fn outside(&self, shown: &str) -> Failure {
        Failure::usage(
            format!(
                "{shown} is outside the folder this server is confined to ({}).",
                self.dir.display()
            ),
            "Give a path inside the root, relative to it, without .. that leaves it or a link that points out of it.",
        )
    }
}

/// A path with `.` dropped and each `..` taking off the name before it, or `None` when a `..` would go above the start.
fn normalize(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => out.push(component),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() || out.as_os_str().is_empty() {
                    return None;
                }
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::Root;
    use std::fs;

    fn root() -> (tempfile::TempDir, Root) {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("inside")).unwrap();
        fs::write(tmp.path().join("inside/a.docx"), "x").unwrap();
        fs::write(tmp.path().join("secret.docx"), "x").unwrap();
        let root = Root::new(&tmp.path().join("inside")).unwrap();
        (tmp, root)
    }

    #[test]
    fn a_relative_path_resolves_inside_the_root() {
        let (_tmp, root) = root();
        assert_eq!(root.resolve("a.docx").unwrap(), root.dir().join("a.docx"));
        assert_eq!(
            root.resolve("./new/b.stet").unwrap(),
            root.dir().join("new/b.stet")
        );
        assert_eq!(
            root.resolve("new/../a.docx").unwrap(),
            root.dir().join("a.docx")
        );
        let absolute = root.dir().join("a.docx");
        assert_eq!(root.resolve(absolute.to_str().unwrap()).unwrap(), absolute);
    }

    #[test]
    fn dot_dot_and_absolute_paths_outside_are_refused() {
        let (tmp, root) = root();
        for escape in [
            "../secret.docx",
            "new/../../secret.docx",
            "..",
            "/etc/passwd",
        ] {
            let error = root.resolve(escape).unwrap_err();
            assert!(error.error().message().contains("outside"), "{escape}");
        }
        let outside = tmp.path().join("secret.docx");
        assert!(root.resolve(outside.to_str().unwrap()).is_err());
        assert!(root.resolve("").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn a_link_that_points_out_is_refused_and_one_that_stays_in_is_followed() {
        let (tmp, root) = root();
        std::os::unix::fs::symlink(tmp.path(), root.dir().join("out")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("secret.docx"), root.dir().join("s.docx"))
            .unwrap();
        std::os::unix::fs::symlink("a.docx", root.dir().join("in.docx")).unwrap();
        std::os::unix::fs::symlink("nowhere", root.dir().join("dangling")).unwrap();
        assert!(root.resolve("out/secret.docx").is_err());
        assert!(
            root.resolve("out/new.docx").is_err(),
            "a new file under a link out"
        );
        assert!(root.resolve("s.docx").is_err());
        assert!(root.resolve("dangling").is_err());
        assert!(root.resolve("dangling/new").is_err());
        assert_eq!(root.resolve("in.docx").unwrap(), root.dir().join("a.docx"));
    }
}
