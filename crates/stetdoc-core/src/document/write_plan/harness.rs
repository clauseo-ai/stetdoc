//! The harness of the write-protocol tests: an in-memory directory that executes a plan, and a scenario that stops the plan
//! after every step, recovers, and checks what is left.

use super::*;
use crate::api::options::OpenOptions;
use crate::document::Document;
use crate::document::complement::DIR;
use std::collections::BTreeMap;

/// A directory: path to bytes, as the CLI would see it.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(super) struct Dir {
    pub(super) paths: BTreeMap<String, Vec<u8>>,
}

pub(super) type Files = BTreeMap<String, String>;
pub(super) type Entries = BTreeMap<String, Vec<u8>>;

pub(super) const DOC_V0: &str = "# one\n\nfirst render\n";
pub(super) const DOC_V1: &str = "# one\n\nsecond render, with more text\n";
pub(super) const NOTES: &str = "[^1]: a note\n";

pub(super) fn files(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
        .collect()
}

pub(super) fn files_of(wc: &WorkingCopy) -> Files {
    wc.files.iter().cloned().collect()
}

pub(super) fn entries_of(wc: &WorkingCopy) -> Entries {
    wc.complement.entries.iter().cloned().collect()
}

impl Dir {
    pub(super) fn of(wc: &WorkingCopy) -> Dir {
        let mut dir = Dir::default();
        for (name, text) in &wc.files {
            dir.paths.insert(name.clone(), text.clone().into_bytes());
        }
        for (name, bytes) in &wc.complement.entries {
            dir.paths.insert(path_of(name), bytes.clone());
        }
        dir
    }

    pub(super) fn apply(&mut self, step: &WriteStep) {
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

    pub(super) fn apply_all(&mut self, steps: &[WriteStep]) {
        for step in steps {
            self.apply(step);
        }
    }

    pub(super) fn set(&mut self, name: &str, text: &str) {
        self.paths
            .insert(name.to_string(), text.as_bytes().to_vec());
    }

    /// The text of a file saved aside, if one of that name and text is there.
    pub(super) fn saved(&self, name: &str, text: &str) -> Option<&String> {
        self.paths
            .iter()
            .find(|(p, b)| {
                p.starts_with(&format!("{name}.")) && p.ends_with(".stale") && *b == text.as_bytes()
            })
            .map(|(p, _)| p)
    }

    /// The working copy as the CLI reads it: the top-level readable files, and every file of `.stet/`, the marker included.
    /// And the marker's bytes, if there is one.
    pub(super) fn read(&self) -> (WorkingCopy, Option<Vec<u8>>) {
        let mut wc = WorkingCopy::default();
        let prefix = format!("{DIR}/");
        for (path, bytes) in &self.paths {
            match path.strip_prefix(&prefix) {
                Some(name) => wc
                    .complement
                    .entries
                    .push((name.to_string(), bytes.clone())),
                None if !path.contains('/') && path.ends_with(".md") => {
                    wc.files
                        .push((path.clone(), String::from_utf8(bytes.clone()).unwrap()));
                }
                None => {}
            }
        }
        (wc, self.paths.get(&path_of(MARKER)).cloned())
    }
}

/// What `recover` leaves, with the CLI's part done: the re-rendered files are written from the new working copy before the
/// marker goes. `None` when the directory has no marker.
pub(super) fn recover_fully(
    dir: &mut Dir,
    new: &WorkingCopy,
) -> Option<Result<Recovery, StetError>> {
    let (wc, marker) = dir.read();
    let recovery = WorkingCopy::recover(&wc, &marker?);
    if let Ok(recovery) = &recovery {
        dir.apply_all(&recovery.steps);
        rewrite(dir, recovery, new);
        dir.apply(&recovery.finish);
    }
    Some(recovery)
}

/// The files of `rerender`, as the new render has them.
pub(super) fn rewrite(dir: &mut Dir, recovery: &Recovery, new: &WorkingCopy) {
    for name in &recovery.rerender {
        let (_, text) = new.files.iter().find(|(n, _)| n == name).unwrap();
        dir.set(name, text);
    }
}

/// One scenario: the working copy on disk (and the hand edits made to it), the one to write, and how.
pub(super) struct Scenario<'a> {
    pub(super) old: Option<(&'a Document, &'a WorkingCopy)>,
    pub(super) hand: &'a [(&'a str, &'a str)],
    pub(super) new: (&'a Document, &'a WorkingCopy),
    pub(super) options: WriteOptions,
}

impl Scenario<'_> {
    pub(super) fn start(&self) -> Dir {
        let mut dir = self.old.map(|(_, wc)| Dir::of(wc)).unwrap_or_default();
        for (name, text) in self.hand {
            dir.set(name, text);
        }
        dir
    }

    pub(super) fn plan(&self) -> Result<WritePlan, StetError> {
        let start = self.start();
        let disk = self.old.map(|_| start.read().0);
        self.new.1.write_plan(disk.as_ref(), &self.options)
    }

    /// Stops the plan after every step and recovers, stopping recovery after every step as well.
    pub(super) fn run(&self) {
        self.crash_everywhere(None);
    }

    /// The same, and the agent edits `name` to `text` after every crash that left a marker, before anything recovers.
    pub(super) fn run_with_edit_after_the_crash(&self, name: &str, text: &str) {
        self.crash_everywhere(Some((name, text)));
    }

    fn crash_everywhere(&self, edit: Option<(&str, &str)>) {
        let new_wc = self.new.1;
        let start = self.start();
        let plan = self.plan().unwrap();
        // The whole plan, uninterrupted, gives the new working copy.
        let mut whole = start.clone();
        whole.apply_all(&plan.steps);
        let (done, marker) = whole.read();
        assert_eq!(marker, None, "the marker is removed last");
        assert_eq!(
            entries_of(&done),
            entries_of(new_wc),
            "entries after the whole plan"
        );
        let mut wanted = files_of(new_wc);
        wanted.extend(self.strays());
        assert_eq!(files_of(&done), wanted, "files after the whole plan");
        for k in 0..=plan.steps.len() {
            let mut dir = start.clone();
            dir.apply_all(&plan.steps[..k]);
            let (_, marker) = dir.read();
            let mut edited = false;
            if marker.is_some() {
                if let Some((name, text)) = edit {
                    dir.set(name, text);
                    edited = true;
                }
                let outcome = recover_fully(&mut dir.clone(), new_wc).unwrap();
                if let Ok(recovery) = &outcome {
                    // A crash inside recovery is recovered by recovering again: after any step, and after the files are
                    // written and before the marker goes.
                    let mut once = dir.clone();
                    let _ = recover_fully(&mut once, new_wc);
                    for j in 0..=recovery.steps.len() + 1 {
                        let mut interrupted = dir.clone();
                        interrupted.apply_all(&recovery.steps[..j.min(recovery.steps.len())]);
                        if j > recovery.steps.len() {
                            rewrite(&mut interrupted, recovery, new_wc);
                        }
                        let _ = recover_fully(&mut interrupted, new_wc);
                        assert_eq!(
                            interrupted, once,
                            "crash at step {k}, then in recovery at step {j}"
                        );
                    }
                }
                let _ = recover_fully(&mut dir, new_wc);
                if let Err(e) = outcome {
                    // Only an interrupted first unpack has nothing to recover.
                    assert!(self.old.is_none(), "crash at step {k}: {e}");
                    assert_eq!(e.code(), ErrorCode::CorruptComplement);
                    assert!(e.message().contains("Delete the directory"), "{e}");
                    assert!(
                        !dir.paths.contains_key(&path_of(MANIFEST)),
                        "crash at step {k}"
                    );
                    continue;
                }
            }
            self.check_settled(&dir, k, edit.filter(|_| edited));
        }
    }

    /// The readable files on disk at the start that are in neither manifest: the agent's own. A write leaves them alone, and so
    /// does its recovery.
    fn strays(&self) -> Files {
        let named = |wc: &WorkingCopy| -> Vec<String> {
            wc.manifest()
                .unwrap()
                .files
                .into_iter()
                .map(|(n, _)| n)
                .collect()
        };
        let in_old = self.old.map(|(_, wc)| named(wc)).unwrap_or_default();
        let in_new = named(self.new.1);
        files_of(&self.start().read().0)
            .into_iter()
            .filter(|(n, _)| !in_old.contains(n) && !in_new.contains(n))
            .collect()
    }

    fn check_settled(&self, dir: &Dir, k: usize, edit: Option<(&str, &str)>) {
        let (wc, marker) = dir.read();
        assert_eq!(marker, None, "crash at step {k}");
        if wc.complement.entries.is_empty() {
            assert!(
                self.old.is_none(),
                "crash at step {k}: the old working copy is gone"
            );
            return;
        }
        let (new_doc, new_wc) = self.new;
        let mut start_files = files_of(&self.start().read().0);
        let strays = self.strays();
        let without_strays = |wc: &WorkingCopy| WorkingCopy {
            files: wc
                .files
                .iter()
                .filter(|(n, _)| !strays.contains_key(n))
                .cloned()
                .collect(),
            complement: wc.complement.clone(),
        };
        let entries = entries_of(&wc);
        let resumed = || {
            Document::resume(&wc, &OpenOptions::default())
                .unwrap_or_else(|e| panic!("crash at step {k}: resume failed: {e}"))
                .base()
        };
        let after = entries == entries_of(new_wc);
        if after {
            assert_eq!(resumed(), new_doc.base(), "crash at step {k}");
            // Nothing is lost: the files are the new render. An edit made after the crash is either still pending, when it
            // was made on the very text the new render has, or it is saved aside and the file is the new render.
            let mut expected = files_of(new_wc);
            expected.extend(strays.clone());
            let clean = without_strays(&wc);
            if let Some((name, text)) = edit {
                let unchanged = start_files.get(name) == expected.get(name);
                if unchanged {
                    expected.insert(name.to_string(), text.to_string());
                    let pending = clean.pending_files().unwrap().unwrap();
                    assert_eq!(pending.files, files(&[(name, text)]), "crash at step {k}");
                    assert_eq!(pending.base, new_doc.base(), "crash at step {k}");
                } else {
                    assert!(
                        dir.saved(name, text).is_some(),
                        "crash at step {k}: the edit made after the crash is lost"
                    );
                    assert_eq!(clean.pending_files().unwrap(), None, "crash at step {k}");
                }
            } else {
                assert_eq!(clean.pending_files().unwrap(), None, "crash at step {k}");
            }
            assert_eq!(
                files_of(&wc),
                expected,
                "crash at step {k}: files of the new working copy"
            );
        } else {
            let (old_doc, old_wc) = self
                .old
                .unwrap_or_else(|| panic!("crash at step {k}: neither old nor new"));
            assert_eq!(
                entries,
                entries_of(old_wc),
                "crash at step {k}: the complement is neither the old nor the new one"
            );
            assert_eq!(resumed(), old_doc.base(), "crash at step {k}");
            // Rolled back: the readable files, hand edits included, are as they were. An edit made after the crash is still
            // pending against the old manifest, as it would have been without the write.
            if let Some((name, text)) = edit {
                start_files.insert(name.to_string(), text.to_string());
            }
            assert_eq!(
                files_of(&wc),
                start_files,
                "crash at step {k}: files of the old working copy"
            );
        }
        // No stale entry is left, whichever way it went.
        let manifest = wc.manifest().unwrap();
        let mut names: Vec<&str> = wc
            .complement
            .entries
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        names.sort_unstable();
        let mut wanted = manifest.entry_names();
        wanted.push(MANIFEST);
        wanted.sort_unstable();
        assert_eq!(names, wanted, "crash at step {k}: stale entries");
    }
}
