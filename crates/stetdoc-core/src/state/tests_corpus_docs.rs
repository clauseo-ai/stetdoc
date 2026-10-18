//! The documents of the corpus, opened as states, for the laws of the cascade and the numbers.
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR` (the set `STETDOC_CORPUS_SET`, `curated`
//! by default), and a test skips when the variable is unset, as the other corpus tests do: `cargo xtask check`
//! always sets it. A document that the manifest says must open (`expect` is `ok`, and it is not in the
//! hostile tier) and that `State::open` refuses is a failure of the test, not a skip.

use super::*;
use stetdoc_testkit::corpus::{Corpus, Entry};

/// True if the manifest says Microsoft Word saved the file (`producer` is `Microsoft Word` and `word_fp` is
/// `y`). `docProps/app.xml` alone is not enough: python-docx's template and other tools write Word-like
/// strings, and a document that Word did not save has no cached numbers that count as Word's.
pub(super) fn saved_by_word(entry: &Entry) -> bool {
    entry.producer == "Microsoft Word" && entry.word_fp == "y"
}

/// Opens every document that must open and gives it to `visit`, one at a time so that only one is in memory.
/// Returns how many it visited, or `None` when `STETDOC_CORPUS_DIR` is not set.
pub(super) fn each_document(mut visit: impl FnMut(&Entry, &State)) -> Option<usize> {
    let corpus = Corpus::from_env()?.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut visited = 0;
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let state = State::open(entry.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open and does not: {e}", entry.id));
        visit(entry, &state);
        visited += 1;
    }
    Some(visited)
}

/// The main part of a state: its name is not always `word/document.xml`.
pub(super) fn main_of(state: &State) -> PartId {
    state
        .main_part()
        .unwrap_or_else(|e| panic!("a state has a main part: {e}"))
}
