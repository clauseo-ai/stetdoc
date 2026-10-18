//! Runs a law over every document of the corpus set on every core, and says what it found.
//!
//! The laws of `laws::*` each take the bytes of a document and return `Err` with a sentence. This runs one over the documents
//! of the set that must open (`expect` ok), and lists the failures sorted by document, so that one run names every document that
//! breaks a law. A document that `Document::open` refuses as malformed input is not a Word document (the container and XML layers
//! accept more than `State::open` does) and is counted as skipped, with its id, and a set in which more than one document in a
//! hundred is skipped is a failure of its own, so that a regression in `open` cannot hide as a skip.

use crate::corpus::Corpus;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use stetdoc_core::{Document, ErrorCode, OpenOptions};

/// Runs `check` on every item on as many threads as the machine has, and returns the failures, sorted. A document is checked on
/// one thread, and a `Document` is `Send`, so the checks do not share anything.
pub fn in_parallel<T: Sync>(
    items: &[T],
    check: impl Fn(&T) -> Option<String> + Sync,
) -> Vec<String> {
    let next = AtomicUsize::new(0);
    let failures = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(items.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    if let Some(why) = check(item)
                        && let Ok(mut list) = failures.lock()
                    {
                        list.push(why);
                    }
                }
            });
        }
    });
    let mut out = failures.into_inner().unwrap_or_default();
    out.sort();
    out
}

/// A failure of a law that is known, and has an owner. A gap is the id of a document, a piece of the failure's sentence, and why it
/// happens and whose it is. The sweep takes it out of the failures, and **a gap that no longer occurs on a document that was
/// checked is a failure of its own**, so that a fix cannot go unnoticed and the list cannot rot.
#[derive(Clone, Copy, Debug)]
pub struct Gap {
    /// The id of the document, `<source>:<path in the source>`.
    pub id: &'static str,
    /// A piece of the sentence that the failure ends in.
    pub says: &'static str,
    /// Why it happens, and who fixes it.
    pub why: &'static str,
}

/// What a sweep of the corpus found.
#[derive(Debug, Default)]
pub struct Swept {
    /// Documents the law ran on.
    pub checked: usize,
    /// The ids of those documents, sorted.
    pub checked_ids: Vec<String>,
    /// Documents that `open` refused as malformed input, by id.
    pub skipped: Vec<String>,
    /// The sentences of the law's failures, with the id of the document, sorted.
    pub failures: Vec<String>,
}

impl Swept {
    /// The sweep without the known gaps. A gap whose document was checked and whose failure did not occur is an error, naming it.
    pub fn minus(mut self, gaps: &[Gap]) -> Result<Swept, String> {
        let mut stale = Vec::new();
        for gap in gaps {
            if self
                .checked_ids
                .binary_search_by(|id| id.as_str().cmp(gap.id))
                .is_err()
            {
                continue;
            }
            let is_it =
                |f: &String| f.starts_with(&format!("{}: ", gap.id)) && f.contains(gap.says);
            if self.failures.iter().any(is_it) {
                self.failures.retain(|f| !is_it(f));
            } else {
                stale.push(format!(
                    "{}: the known gap ({}) no longer occurs, so delete it from the list",
                    gap.id, gap.why
                ));
            }
        }
        if stale.is_empty() {
            Ok(self)
        } else {
            Err(stale.join("\n"))
        }
    }

    /// `Ok` when the law held for every document, enough documents were checked, and the skips are few.
    pub fn verdict(&self, at_least: usize) -> Result<(), String> {
        let mut problems = Vec::new();
        if self.checked < at_least {
            problems.push(format!(
                "only {} documents were checked, {at_least} expected",
                self.checked
            ));
        }
        if self.skipped.len() * 100 > self.checked + self.skipped.len() {
            problems.push(format!(
                "{} documents were skipped as malformed input, over one in a hundred: {}",
                self.skipped.len(),
                self.skipped.join(", ")
            ));
        }
        if !self.failures.is_empty() {
            problems.push(format!(
                "{} failures:\n{}",
                self.failures.len(),
                self.failures.join("\n")
            ));
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n"))
        }
    }
}

/// Runs `law` over the documents of the corpus set that must open. `None` when `STETDOC_CORPUS_DIR` is not set.
pub fn over_the_corpus(
    law: impl Fn(&[u8]) -> Result<(), String> + Sync,
) -> Option<Result<Swept, String>> {
    let corpus = match Corpus::from_env()? {
        Ok(corpus) => corpus,
        Err(e) => return Some(Err(e)),
    };
    let entries: Vec<_> = corpus.expected_ok().collect();
    let skipped = Mutex::new(Vec::new());
    let checked = Mutex::new(Vec::new());
    let failures = in_parallel(&entries, |entry| {
        let bytes = entry.bytes();
        match Document::open(bytes.clone(), &OpenOptions::default()) {
            Ok(_) => {}
            Err(e) if e.code() == ErrorCode::MalformedInput => {
                if let Ok(mut list) = skipped.lock() {
                    list.push(entry.id.clone());
                }
                return None;
            }
            Err(e) => return Some(format!("{}: open: {e}", entry.id)),
        }
        if let Ok(mut list) = checked.lock() {
            list.push(entry.id.clone());
        }
        law(&bytes).err().map(|why| format!("{}: {why}", entry.id))
    });
    let mut skipped = skipped.into_inner().unwrap_or_default();
    skipped.sort();
    let mut checked_ids = checked.into_inner().unwrap_or_default();
    checked_ids.sort();
    Some(Ok(Swept {
        checked: checked_ids.len(),
        checked_ids,
        skipped,
        failures,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn swept(failures: &[&str]) -> Swept {
        Swept {
            checked: 2,
            checked_ids: vec!["a:one.docx".into(), "b:two.docx".into()],
            skipped: Vec::new(),
            failures: failures.iter().map(|f| (*f).to_string()).collect(),
        }
    }

    const GAP: Gap = Gap {
        id: "a:one.docx",
        says: "textId is not an attribute",
        why: "a test",
    };

    #[test]
    fn a_known_gap_is_taken_out_of_the_failures_and_nothing_else() {
        let found = swept(&[
            "a:one.docx: {\"op\":\"insert\"}: validation_failed: p1: textId is not an attribute of w:p.",
            "b:two.docx: something else",
        ]);
        let left = found.minus(&[GAP]).unwrap();
        assert_eq!(left.failures, ["b:two.docx: something else"]);
    }

    #[test]
    fn a_gap_that_no_longer_occurs_on_a_document_that_was_checked_is_an_error() {
        let e = swept(&[]).minus(&[GAP]).unwrap_err();
        assert!(
            e.contains("a:one.docx") && e.contains("no longer occurs"),
            "{e}"
        );
        // The same message about another document is not the gap.
        let e = swept(&["b:two.docx: textId is not an attribute"])
            .minus(&[GAP])
            .unwrap_err();
        assert!(e.contains("no longer occurs"), "{e}");
    }

    #[test]
    fn a_gap_of_a_document_that_was_not_checked_is_not_asked_for() {
        let other = Gap {
            id: "c:three.docx",
            ..GAP
        };
        assert!(swept(&[]).minus(&[other]).is_ok());
    }

    #[test]
    fn the_verdict_counts_what_was_checked_skipped_and_failed() {
        let mut found = swept(&[]);
        assert!(found.verdict(2).is_ok());
        assert!(found.verdict(3).is_err());
        found.skipped = vec!["x".into(); 3];
        assert!(found.verdict(2).unwrap_err().contains("skipped"));
        found.skipped.clear();
        found.failures = vec!["b:two.docx: broke".into()];
        assert!(found.verdict(2).unwrap_err().contains("1 failures"));
    }
}
