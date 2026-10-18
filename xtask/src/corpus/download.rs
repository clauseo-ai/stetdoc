//! Downloads the missing documents, one source per worker, and turns the failures into
//! short messages.
//!
//! When a source fails because the remote cannot be reached or does not have the pinned
//! commit, no other source is started and the result is one line. Documents that do not
//! match the manifest are all listed, one per line, grouped by source.

use crate::corpus::cache::{self, ENV};
use crate::corpus::gitfetch::{self, FetchError};
use crate::corpus::manifest::{Doc, Manifest};
use crate::repo::Res;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Downloads `missing` into the blobs of `root`, keeping the git repositories under
/// `work_root`. `wanted` is how many documents the fetch needs in all, for the message.
pub fn run(
    m: &Manifest,
    root: &Path,
    work_root: &Path,
    missing: &[&Doc],
    wanted: usize,
    jobs: usize,
) -> Res<()> {
    let mut by_source: BTreeMap<&str, Vec<&Doc>> = BTreeMap::new();
    for d in missing {
        by_source.entry(d.source.as_str()).or_default().push(d);
    }
    let mut queue: Vec<(&str, Vec<&Doc>)> = by_source.into_iter().collect();
    queue.sort_by_key(|(_, docs)| docs.len());
    let queue = Mutex::new(queue);
    let abort = AtomicBool::new(false);
    let stopped: Mutex<Option<(String, FetchError)>> = Mutex::new(None);
    let errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let workers = if jobs == 0 { 4 } else { jobs };
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while !abort.load(Ordering::SeqCst) {
                    let next = queue.lock().ok().and_then(|mut q| q.pop());
                    let Some((name, docs)) = next else { return };
                    let Some(source) = m.source(name) else {
                        record(
                            &errors,
                            format!("source `{name}` is not in corpus/sources.tsv"),
                        );
                        continue;
                    };
                    let work = work_root.join(name);
                    let result = gitfetch::fetch(
                        &work,
                        &source.url,
                        &source.commit,
                        &docs,
                        &abort,
                        |d, bytes| cache::store(root, d, bytes),
                    );
                    let _ = std::fs::remove_dir_all(&work);
                    match result {
                        Ok(()) => {}
                        Err(e) if e.stops_everything() => {
                            if let Ok(mut first) = stopped.lock()
                                && first.is_none()
                            {
                                *first = Some((name.to_string(), e));
                            }
                        }
                        Err(e) => record(&errors, source_failure(name, &e)),
                    }
                }
            });
        }
    });
    let stopped = stopped.into_inner().ok().flatten();
    if let Some((name, error)) = stopped {
        let still_missing = missing
            .iter()
            .filter(|d| !cache::have(root, d, false))
            .count();
        return Err(one_line(root, &name, &error, still_missing, wanted));
    }
    let errors = errors.into_inner().unwrap_or_default();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} source(s) failed:\n{}",
            errors.len(),
            errors.join("\n")
        ))
    }
}

/// The message for a source that failed without stopping the others. A repository that is
/// gone says what to do: the pin cannot be fetched, so it must be replaced.
fn source_failure(name: &str, error: &FetchError) -> String {
    match error {
        FetchError::Gone { url } => format!(
            "source `{name}`: the repository {url} was not found (deleted, renamed or made private): \
             replace its URL and pinned commit in corpus/sources.tsv with a copy that exists, or drop the source and its documents"
        ),
        other => format!("source `{name}`:\n{other}"),
    }
}

fn record(errors: &Mutex<Vec<String>>, message: String) {
    if let Ok(mut errs) = errors.lock() {
        errs.push(message);
    }
}

/// The single line for a failure that stops the whole fetch.
fn one_line(
    root: &Path,
    source: &str,
    error: &FetchError,
    missing: usize,
    wanted: usize,
) -> String {
    match error {
        FetchError::Unreachable { url, detail } => format!(
            "cannot reach {url} ({detail}): the corpus cache at {} is missing {missing} of {wanted} documents; \
             connect to the network and run `cargo xtask corpus fetch`, or point {ENV} at a populated cache",
            root.display()
        ),
        FetchError::BadPin { url, commit } => format!(
            "the pinned commit {commit} is not available from {url} (source {source}): \
             the pin in corpus/sources.tsv is wrong or the repository was rewritten"
        ),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stopping_errors_make_one_line_each() {
        let root = Path::new("/cache");
        let unreachable = FetchError::Unreachable {
            url: "https://x/y".into(),
            detail: "fatal: no route".into(),
        };
        let line = one_line(root, "demo", &unreachable, 3, 7);
        assert!(!line.contains('\n'), "{line}");
        assert!(line.starts_with("cannot reach https://x/y (fatal: no route): "));
        assert!(
            line.contains("/cache is missing 3 of 7 documents"),
            "{line}"
        );
        assert!(line.contains("`cargo xtask corpus fetch`"), "{line}");
        assert!(line.contains(ENV), "{line}");
        let gone = source_failure(
            "demo",
            &FetchError::Gone {
                url: "https://x/y".into(),
            },
        );
        assert!(!gone.contains('\n'), "{gone}");
        assert!(gone.contains("replace its URL and pinned commit"), "{gone}");
        assert!(gone.starts_with("source `demo`: the repository https://x/y was not found"));
        let pin = FetchError::BadPin {
            url: "https://x/y".into(),
            commit: "c0ffee".into(),
        };
        let line = one_line(root, "demo", &pin, 3, 7);
        assert!(!line.contains('\n'), "{line}");
        assert!(
            line.starts_with(
                "the pinned commit c0ffee is not available from https://x/y (source demo)"
            ),
            "{line}"
        );
    }
}
