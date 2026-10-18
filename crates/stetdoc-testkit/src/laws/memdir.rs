//! An in-memory directory that executes a `WritePlan`, so a test can stop it after any step.

use std::collections::BTreeMap;
use stetdoc_core::{Complement, WorkingCopy, WriteStep};

/// The directory of a working copy: relative path to bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemDir {
    /// Every file, by path relative to the working copy's directory.
    pub paths: BTreeMap<String, Vec<u8>>,
}

impl MemDir {
    /// An empty directory.
    pub fn new() -> MemDir {
        MemDir::default()
    }

    /// The directory that holds a working copy, fully written.
    pub fn of(working_copy: &WorkingCopy) -> MemDir {
        let mut dir = MemDir::new();
        for (name, text) in &working_copy.files {
            dir.paths.insert(name.clone(), text.clone().into_bytes());
        }
        for (name, bytes) in &working_copy.complement.entries {
            dir.paths.insert(format!(".stet/{name}"), bytes.clone());
        }
        dir
    }

    /// Executes one step. A write is whole, as an atomic rename makes it, and removing a file that is gone is not an error.
    pub fn apply(&mut self, step: &WriteStep) {
        match step {
            WriteStep::Marker { path, bytes }
            | WriteStep::Entry { path, bytes }
            | WriteStep::Commit { path, bytes } => {
                self.paths.insert(path.clone(), bytes.clone());
            }
            WriteStep::File { path, text } => {
                self.paths.insert(path.clone(), text.clone().into_bytes());
            }
            WriteStep::Remove { path } => {
                self.paths.remove(path);
            }
        }
    }

    /// Executes steps in order.
    pub fn apply_all<'a>(&mut self, steps: impl IntoIterator<Item = &'a WriteStep>) {
        for step in steps {
            self.apply(step);
        }
    }

    /// The working copy on disk now, as the CLI reads it: the top-level `.md` files and every file of `.stet/`, the marker
    /// (the entry `writing`) included, so that the other methods see an interrupted write. And the marker's bytes, if there is
    /// one, for `WorkingCopy::recover`.
    pub fn read(&self) -> (WorkingCopy, Option<Vec<u8>>) {
        let mut working_copy = WorkingCopy {
            files: Vec::new(),
            complement: Complement::default(),
        };
        for (path, bytes) in &self.paths {
            match path.strip_prefix(".stet/") {
                Some(name) => working_copy
                    .complement
                    .entries
                    .push((name.to_string(), bytes.clone())),
                None if !path.contains('/') && path.ends_with(".md") => {
                    working_copy
                        .files
                        .push((path.clone(), String::from_utf8_lossy(bytes).into_owned()));
                }
                None => {}
            }
        }
        (working_copy, self.paths.get(".stet/writing").cloned())
    }
}
