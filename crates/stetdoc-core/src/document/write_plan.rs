//! Writing a working copy to disk, and surviving a crash.
//!
//! The core does no I/O, and the order of the writes is still the core's, because the order is what makes a crash harmless.
//! `WorkingCopy::write_plan` returns the steps and the CLI executes them one at a time, each as a temporary file, an fsync and
//! an atomic rename, so a file is whole or absent. **The one commit point is the rename of the manifest.** Until then the old
//! manifest names the old entries, which no step has touched, and after it every readable file is either unmodified, rewritten
//! or left alone. `recover` finishes or undoes an interrupted write without losing a hand edit.
//!
//! The order is: the marker `.stet/writing`; the new entries (names the old manifest does not have, so none overwrites one it
//! names); the manifest; the readable files; the removals (readable files the render dropped, stale entries, and the marker
//! last). A hand edit is never overwritten: a plan that would is refused with `pending_edits` before its first step.

use super::complement::{MANIFEST, MARKER, Manifest, is_generated, path_of};
use crate::api::error::{ErrorCode, StetError};
use crate::api::working_copy::{Recovery, WorkingCopy, WriteOptions, WritePlan, WriteStep};
use crate::refs::{hex, sha256, unhex};
use std::collections::{BTreeMap, BTreeSet};

fn corrupt(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::CorruptComplement, message)
}

/// What the marker records: the generations the write came from and goes to, the SHA-256 of the manifest that was on disk and
/// of the one being committed (a refresh export at one generation has two manifests of one generation, so the digest is what
/// tells them apart), the readable files that the plan removes (a file that is on disk and in no manifest is the agent's, and
/// a write leaves it alone, so recovery may remove only what the plan said it would), and the readable files as they were on
/// disk.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Marker {
    from: Option<u32>,
    to: u32,
    old: Option<[u8; 32]>,
    new: [u8; 32],
    drops: Vec<String>,
    files: Vec<(String, [u8; 32])>,
}

impl Marker {
    fn render(&self) -> String {
        let from = self.from.map_or("none".to_string(), |g| g.to_string());
        let old = self.old.map_or("none".to_string(), |d| hex(&d));
        let mut out = format!(
            "stetdoc-writing 1\nfrom {from}\nto {}\nold-manifest {old}\nnew-manifest {}\n",
            self.to,
            hex(&self.new)
        );
        for name in &self.drops {
            out.push_str(&format!("drop {name}\n"));
        }
        for (name, sha) in &self.files {
            out.push_str(&format!("file {name} {}\n", hex(sha)));
        }
        out
    }

    fn parse(bytes: &[u8]) -> Result<Marker, StetError> {
        let bad = || corrupt("`.stet/writing` is not an interruption marker of this release.");
        let text = std::str::from_utf8(bytes).map_err(|_| bad())?;
        let mut lines = text.lines();
        if lines.next() != Some("stetdoc-writing 1") {
            return Err(bad());
        }
        let number = |line: Option<&str>, key: &str| -> Option<String> {
            line?
                .strip_prefix(key)?
                .strip_prefix(' ')
                .map(str::to_string)
        };
        let from = number(lines.next(), "from").ok_or_else(bad)?;
        let from = if from == "none" {
            None
        } else {
            Some(from.parse::<u32>().map_err(|_| bad())?)
        };
        let to = number(lines.next(), "to")
            .and_then(|t| t.parse::<u32>().ok())
            .ok_or_else(bad)?;
        let old = number(lines.next(), "old-manifest").ok_or_else(bad)?;
        let old = if old == "none" {
            None
        } else {
            Some(unhex::<32>(&old).ok_or_else(bad)?)
        };
        let new = number(lines.next(), "new-manifest")
            .and_then(|d| unhex::<32>(&d))
            .ok_or_else(bad)?;
        let mut drops = Vec::new();
        let mut files = Vec::new();
        for line in lines {
            let mut words = line.split(' ');
            match (words.next(), words.next(), words.next(), words.next()) {
                (Some("drop"), Some(name), None, None) if files.is_empty() => {
                    drops.push(name.to_string());
                }
                (Some("file"), Some(name), Some(sha), None) => {
                    files.push((name.to_string(), unhex::<32>(sha).ok_or_else(bad)?));
                }
                _ => return Err(bad()),
            }
        }
        Ok(Marker {
            from,
            to,
            old,
            new,
            drops,
            files,
        })
    }
}

impl WorkingCopy {
    /// The steps that write this working copy over `disk`, the working copy as it is on disk now (`None` for the first unpack).
    ///
    /// A readable file is overwritten only if it is unmodified (its text is what the old manifest recorded), missing, named in
    /// `options.consumed` (its hand edits went into this export), or `options.refresh` is set. Any other differing file holds a
    /// hand edit that this export did not take in, and the plan is refused with `pending_edits`.
    pub fn write_plan(
        &self,
        disk: Option<&WorkingCopy>,
        options: &WriteOptions,
    ) -> Result<WritePlan, StetError> {
        let new = self.manifest()?;
        for name in new.entry_names() {
            if self.complement.entry(name)?.is_none() {
                return Err(StetError::new(
                    ErrorCode::Internal,
                    format!(
                        "internal: the working copy has no entry `{name}`, which its manifest names."
                    ),
                ));
            }
        }
        let old = disk.map(WorkingCopy::manifest).transpose()?;
        if let Some(old) = &old {
            if old.original != new.original {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    "The directory holds the working copy of another document. Use an empty directory.",
                ));
            }
            // An entry is not written again when the old manifest has its name, so the name must stand for the same bytes.
            // The name holds eight hex digits of the digest, and the manifest the whole of it: a collision is refused.
            for name in new.entry_names() {
                if old
                    .digest_of(name)
                    .is_some_and(|d| Some(d) != new.digest_of(name))
                {
                    return Err(StetError::new(
                        ErrorCode::Internal,
                        format!(
                            "internal: the entry `{name}` has the same name in the old and the new manifest and different bytes, which a short-digest collision can do."
                        ),
                    )
                    .with_next("Unpack the document into a new directory."));
                }
            }
        }
        let disk_files: BTreeMap<&str, &str> = disk
            .map(|d| {
                d.files
                    .iter()
                    .map(|(n, t)| (n.as_str(), t.as_str()))
                    .collect()
            })
            .unwrap_or_default();
        let old_hash: BTreeMap<&str, [u8; 32]> = old
            .as_ref()
            .map(|m| m.files.iter().map(|(n, s)| (n.as_str(), *s)).collect())
            .unwrap_or_default();
        // May this file be overwritten or removed?
        let free = |name: &str, text: &str| {
            options.refresh
                || options.consumed.iter().any(|c| c == name)
                || old_hash
                    .get(name)
                    .is_some_and(|h| *h == sha256(text.as_bytes()))
        };
        let mut file_steps = Vec::new();
        for (name, text) in &self.files {
            match disk_files.get(name.as_str()) {
                Some(on_disk) if *on_disk == text => continue,
                Some(on_disk) if !free(name, on_disk) => return Err(hand_edit(name)),
                _ => file_steps.push(WriteStep::File {
                    path: name.clone(),
                    text: text.clone(),
                }),
            }
        }
        // The files that the old manifest wrote and the new render does not have go. A file in neither manifest is the agent's
        // own, and a write leaves it alone.
        let mut dropped = Vec::new();
        let mut drops = Vec::new();
        for (name, on_disk) in &disk_files {
            let rendered = self.files.iter().any(|(n, _)| n == name);
            if rendered || !old_hash.contains_key(name) {
                continue;
            }
            if !free(name, on_disk) {
                return Err(hand_edit(name));
            }
            drops.push((*name).to_string());
            dropped.push(WriteStep::Remove {
                path: (*name).to_string(),
            });
        }

        let manifest_bytes = |wc: &WorkingCopy| -> Result<Vec<u8>, StetError> {
            Ok(wc.complement.entry(MANIFEST)?.unwrap_or_default().to_vec())
        };
        let new_bytes = manifest_bytes(self)?;
        let marker = Marker {
            from: old.as_ref().map(|m| m.generation),
            to: new.generation,
            old: match disk {
                Some(d) if old.is_some() => Some(sha256(&manifest_bytes(d)?)),
                _ => None,
            },
            new: sha256(&new_bytes),
            drops,
            files: {
                let mut files: Vec<(String, [u8; 32])> = disk_files
                    .iter()
                    .map(|(n, t)| ((*n).to_string(), sha256(t.as_bytes())))
                    .collect();
                files.sort();
                files
            },
        };
        let mut steps = vec![WriteStep::Marker {
            path: path_of(MARKER),
            bytes: marker.render().into_bytes(),
        }];
        let have: BTreeSet<&str> = old
            .as_ref()
            .map(Manifest::entry_names)
            .unwrap_or_default()
            .into_iter()
            .collect();
        for name in new.entry_names() {
            if !have.contains(name) {
                let bytes = self.complement.entry(name)?.unwrap_or_default().to_vec();
                steps.push(WriteStep::Entry {
                    path: path_of(name),
                    bytes,
                });
            }
        }
        steps.push(WriteStep::Commit {
            path: path_of(MANIFEST),
            bytes: new_bytes,
        });
        steps.extend(file_steps);
        steps.extend(dropped);
        let keep: BTreeSet<&str> = new.entry_names().into_iter().collect();
        for name in have {
            if !keep.contains(name) {
                steps.push(WriteStep::Remove {
                    path: path_of(name),
                });
            }
        }
        steps.push(WriteStep::Remove {
            path: path_of(MARKER),
        });
        Ok(WritePlan { steps })
    }

    /// The steps that finish or undo an interrupted write, for a directory that has `.stet/writing`. `disk` is the working copy
    /// as it is on disk now, the entry `writing` included (the other methods refuse a working copy that has it, so that nothing
    /// reads the directory in the middle of a write), and `marker` is the bytes of that entry. It is called before anything
    /// else touches the directory.
    ///
    /// - The manifest is the one the write started from: **roll back**. The readable files were not touched, so every hand edit
    ///   is intact and still pending, and the entries that the manifest does not name go.
    /// - The manifest is the one the write was committing: **finish**. The entries that the manifest does not name go. A readable
    ///   file whose text is the new render is done. One whose text is what the marker recorded, or one that is missing, is listed
    ///   in `rerender`, for the caller to render again from the committed head.
    ///
    ///   **A file written after the crash** has text that is neither. If the marker recorded the very text that the new manifest
    ///   records (the file did not change with this write), the edit is against the new render and stays pending (`kept`).
    ///   Otherwise the edit was made on a render that the commit has replaced, and a pending edit would read the committed
    ///   change as the agent's own and undo it. So the text is **saved aside** as `<name>.<8 hex digits of its digest>.stale`,
    ///   which has no readable extension and so is never a readable file, the file is listed in `rerender`, and a warning names
    ///   the saved file and tells the agent to redo the edit on the fresh one. The same is done to a file that the new render
    ///   no longer has, which is then removed.
    /// - Anything else is `corrupt_complement`.
    ///
    /// Every step is idempotent, and a name stands for one text, so a crash during recovery is recovered by running it again.
    /// A caller prints the warnings before it runs the steps.
    pub fn recover(disk: &WorkingCopy, marker: &[u8]) -> Result<Recovery, StetError> {
        let marker = Marker::parse(marker)?;
        let manifest = disk.read_manifest().map_err(|e| match e.code() {
            ErrorCode::CorruptComplement if disk.complement.entry(MANIFEST).ok().flatten().is_none() => corrupt(
                "A write was interrupted before the manifest was written, so there is no working copy to recover. \
                 Delete the directory and unpack the document again.",
            ),
            _ => e,
        })?;
        let on_disk_manifest = sha256(disk.complement.entry(MANIFEST)?.unwrap_or_default());
        let finishing = if on_disk_manifest == marker.new {
            true
        } else if Some(on_disk_manifest) == marker.old {
            false
        } else {
            return Err(corrupt(format!(
                "The manifest at generation {} is neither the one the interrupted write started from ({}) nor the one it was committing (generation {}).",
                manifest.generation,
                marker
                    .from
                    .map_or("nothing".to_string(), |g| format!("generation {g}")),
                marker.to
            )));
        };
        let keep: BTreeSet<&str> = manifest.entry_names().into_iter().collect();
        let mut steps = Vec::new();
        let mut stale: Vec<&str> = disk
            .complement
            .entries
            .iter()
            .map(|(n, _)| n.as_str())
            .filter(|n| is_generated(n) && !keep.contains(n))
            .collect();
        stale.sort_unstable();
        stale.dedup();
        for name in stale {
            steps.push(WriteStep::Remove {
                path: path_of(name),
            });
        }
        let mut rerender = Vec::new();
        let mut kept = Vec::new();
        let mut warnings = Vec::new();
        if finishing {
            let recorded: BTreeMap<&str, [u8; 32]> =
                marker.files.iter().map(|(n, s)| (n.as_str(), *s)).collect();
            let new: BTreeMap<&str, [u8; 32]> = manifest
                .files
                .iter()
                .map(|(n, s)| (n.as_str(), *s))
                .collect();
            let on_disk: BTreeMap<&str, &str> = disk
                .files
                .iter()
                .map(|(n, t)| (n.as_str(), t.as_str()))
                .collect();
            for (name, sha) in &new {
                let Some(text) = on_disk.get(name) else {
                    rerender.push((*name).to_string());
                    continue;
                };
                let here = sha256(text.as_bytes());
                if here == *sha {
                    continue;
                }
                if recorded.get(name) == Some(&here) {
                    rerender.push((*name).to_string());
                } else if recorded.get(name) == Some(sha) {
                    kept.push((*name).to_string());
                } else {
                    let saved = saved_name(name, &here);
                    warnings.push(format!(
                        "`{name}` was edited after an interrupted write that had already committed a newer version, so the edit is against a render that the commit replaced. The edit is saved as `{saved}`, which is not a readable file, and `{name}` is rendered again from the committed version. Redo the edit on `{name}`."
                    ));
                    steps.push(WriteStep::File {
                        path: saved,
                        text: (*text).to_string(),
                    });
                    rerender.push((*name).to_string());
                }
            }
            // Only the files that the plan said it would remove: one that is on disk and in no manifest is the agent's.
            for name in &marker.drops {
                let Some(text) = on_disk.get(name.as_str()) else {
                    continue;
                };
                if new.contains_key(name.as_str()) {
                    continue;
                }
                let here = sha256(text.as_bytes());
                if recorded.get(name.as_str()) != Some(&here) {
                    let saved = saved_name(name, &here);
                    warnings.push(format!(
                        "`{name}` is no longer part of the working copy, and it was edited after an interrupted write. The edit is saved as `{saved}`, which is not a readable file, and `{name}` is removed."
                    ));
                    steps.push(WriteStep::File {
                        path: saved,
                        text: (*text).to_string(),
                    });
                }
                steps.push(WriteStep::Remove { path: name.clone() });
            }
        }
        Ok(Recovery {
            steps,
            finish: WriteStep::Remove {
                path: path_of(MARKER),
            },
            rerender,
            kept,
            warnings,
        })
    }
}

/// Where the text of a file is saved aside: its name, eight hex digits of its digest, and an extension no format reads. Two
/// texts never share a name, and one text always has the same, so saving it again is harmless.
fn saved_name(name: &str, sha: &[u8; 32]) -> String {
    let digest = hex(sha);
    format!("{name}.{}.stale", digest.get(..8).unwrap_or(&digest))
}

fn hand_edit(name: &str) -> StetError {
    StetError::new(
        ErrorCode::PendingEdits,
        format!("`{name}` holds hand edits that this write did not take in, and writing would overwrite them."),
    )
    .with_next("Pack the hand edits first, or export again with `--refresh` to discard them.")
}

#[cfg(test)]
mod harness;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_recover;
