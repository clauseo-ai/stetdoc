//! The laws that need real documents, over the corpus named by `STETDOC_CORPUS_DIR` (design 8.2c): on every
//! document that opens and holds revisions, `changes()` puts each revision element in exactly one change,
//! accepting all reads as `Final` and rejecting all as `Original` in every story, resolving twice changes
//! nothing, no revision is left, and resolution adds no schema violation to the main part. With the variable
//! unset the test skips and passes, as the other corpus tests of the core do.

use super::changes::changes;
use super::index::{ChangeIndex, revision_parts};
use super::laws_util::{final_runs_in, runs_in};
use super::notes::RemovedNote;
use super::resolve::resolve;
use super::tests_support::{MAIN, schema_added};
use super::types::{Action, Selector, Step};
use crate::api::limits::Limits;
use crate::state::{State, Tx};
use crate::wordml::{Mode, part_revisions};
use std::collections::{BTreeMap, HashSet};
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::expect::Expect;

/// The parts of a state that hold revisions, with how many each holds, and which of them are stories.
fn parts_with_revisions(tx: &Tx<'_>) -> Result<Vec<(std::sync::Arc<str>, usize, bool)>, String> {
    let view = tx.parts();
    let mut out = Vec::new();
    for name in revision_parts(tx).map_err(|e| format!("revision parts: {e}"))? {
        let Some(id) = view.id(&name) else { continue };
        let tree = view.tree(id).map_err(|e| format!("{name}: {e}"))?;
        let n = part_revisions(tree).len();
        if n > 0 {
            out.push((name.clone(), n, view.story(id).is_ok()));
        }
    }
    Ok(out)
}

/// Resolves everything, and returns the notes that went because their last reference did.
fn resolve_all(tx: &mut Tx<'_>, action: Action) -> Result<Vec<RemovedNote>, String> {
    let step = Step {
        action,
        selector: Selector::All,
    };
    resolve(tx, &mut ChangeIndex::new(), &[step])
        .map(|steps| steps.into_iter().flat_map(|s| s.notes).collect())
        .map_err(|e| format!("{action:?} all failed: {e}"))
}

fn bytes_of(tx: &Tx<'_>, parts: &[(std::sync::Arc<str>, usize, bool)]) -> Vec<Vec<u8>> {
    let view = tx.parts();
    parts
        .iter()
        .filter_map(|(name, _, _)| view.id(name).and_then(|id| view.tree(id).ok()))
        .map(|t| t.serialize())
        .collect()
}

/// Checks one document. `Ok(false)` if it does not open or holds no revision.
fn check(bytes: Vec<u8>) -> Result<bool, String> {
    let Ok(state) = State::open(bytes, &Limits::default()) else {
        return Ok(false);
    };
    let tx = state.begin();
    let parts = parts_with_revisions(&tx)?;
    if parts.is_empty() {
        return Ok(false);
    }
    // Every revision element is in exactly one change.
    {
        let view = state.parts();
        for (name, n, story) in &parts {
            if !story {
                continue;
            }
            let (Some(id), true) = (view.id(name), true) else {
                continue;
            };
            let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
                continue;
            };
            let cs = changes(&story, tree, None);
            let nodes: Vec<_> = cs.iter().flat_map(|c| c.nodes.iter().copied()).collect();
            let distinct: HashSet<_> = nodes.iter().copied().collect();
            if nodes.len() != distinct.len() || nodes.len() != *n {
                return Err(format!(
                    "{name}: {n} revision elements, {} in changes, {} distinct",
                    nodes.len(),
                    distinct.len()
                ));
            }
        }
    }
    // Reading by window and by id agrees with reading the whole story.
    super::laws_select::check_windows(&state)?;
    super::laws_select::check_ids(&state)?;
    let story_parts: Vec<&std::sync::Arc<str>> = parts
        .iter()
        .filter(|(_, _, story)| *story)
        .map(|(n, _, _)| n)
        .collect();
    for (action, label) in [(Action::Accept, "accept"), (Action::Reject, "reject")] {
        let mut tx = state.begin();
        let removed_notes = resolve_all(&mut tx, action)?;
        let notes_of = |name: &str| -> Vec<i64> {
            removed_notes
                .iter()
                .filter(|n| &*n.part == name)
                .map(|n| n.id)
                .collect()
        };
        // Nothing is left.
        let left = parts_with_revisions(&tx)?;
        if !left.is_empty() {
            let view = tx.parts();
            let mut what = Vec::new();
            for (name, _, _) in &left {
                if let Some(id) = view.id(name)
                    && let Ok(tree) = view.tree(id)
                {
                    for r in part_revisions(tree).iter().take(3) {
                        let parent = tree
                            .node_ref(r.node)
                            .parent()
                            .map(|p| p.local_name().to_owned())
                            .unwrap_or_default();
                        what.push(format!("{name}: {:?} id {:?} under {parent}", r.kind, r.id));
                    }
                }
            }
            return Err(format!("{label} all left revisions: {what:?}"));
        }
        // Every story reads as the mode says.
        for name in &story_parts {
            let mode = match action {
                Action::Accept => Mode::Final,
                Action::Reject => Mode::Original,
            };
            let before = runs_in(&state.parts(), name, mode, true, &notes_of(name));
            let after = final_runs_in(&tx.parts(), name);
            if before != after {
                let at = before
                    .iter()
                    .zip(after.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(before.len().min(after.len()));
                return Err(format!(
                    "{label} all: {name} does not read as {}: {} paragraphs before, {} after, first difference at {at}: {:?} against {:?}",
                    if action == Action::Accept {
                        "Final"
                    } else {
                        "Original"
                    },
                    before.len(),
                    after.len(),
                    before.get(at),
                    after.get(at)
                ));
            }
        }
        // The text, with nothing left out for where it sits, is the same: only paragraphs with no text
        // differ, which are those of rows and cells that a tracked change removes, and of block-level
        // wrappers that remove them, which `wordml` lists as empty entries and the resolution removes.
        for name in &story_parts {
            let skipped = notes_of(name);
            let strict = |view: &crate::state::PartsView<'_>, mode| {
                runs_in(view, name, mode, false, &skipped)
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
            };
            let (mode, want) = match action {
                Action::Accept => (Mode::Final, "Final"),
                Action::Reject => (Mode::Original, "Original"),
            };
            let (before, after) = (
                strict(&state.parts(), mode),
                strict(&tx.parts(), Mode::Final),
            );
            if before != after {
                let at = before
                    .iter()
                    .zip(after.iter())
                    .position(|(a, b)| a != b)
                    .unwrap_or(before.len().min(after.len()));
                return Err(format!(
                    "{label} all: the text of {name} is not that of {want}: {} paragraphs with text before, {} after, first difference at {at}: {:?} against {:?}",
                    before.len(),
                    after.len(),
                    before.get(at),
                    after.get(at)
                ));
            }
        }
        // The main part has no violation that the document did not have.
        let added = schema_added(&state, &tx);
        if !added.is_empty() {
            return Err(format!(
                "{label} all adds schema violations to {MAIN}: {added:?}"
            ));
        }
        // Twice changes nothing.
        let once = bytes_of(&tx, &parts);
        resolve_all(&mut tx, action)?;
        if bytes_of(&tx, &parts) != once {
            return Err(format!("{label} all twice changed the document"));
        }
    }
    Ok(true)
}

#[test]
fn resolving_every_corpus_document_is_sound() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut failures: BTreeMap<String, String> = BTreeMap::new();
    let mut checked = 0;
    for entry in corpus.entries.iter().filter(|e| e.expect == Expect::Ok) {
        match check(entry.bytes()) {
            Ok(true) => checked += 1,
            Ok(false) => {}
            Err(why) => {
                failures.insert(entry.id.clone(), why);
            }
        }
    }
    eprintln!(
        "{checked} documents with revisions checked, {} failed",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} documents broke a law:\n{}",
        failures.len(),
        failures
            .iter()
            .map(|(id, why)| format!("{id}: {why}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The batches of the laws on corpus documents: an even sample of the documents that open and are small
/// enough for the laws to be run on in a test build, each with two sequences of two batches. A failure is
/// the document's id and the message of the law, and the run goes on to the next document.
#[test]
fn the_laws_hold_for_batches_on_corpus_documents() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let small: Vec<_> = corpus
        .entries
        .iter()
        .filter(|e| e.expect == Expect::Ok)
        .filter(|e| e.bytes().len() <= 120_000)
        .collect();
    let stride = (small.len() / 100).max(1);
    let mut failures: BTreeMap<String, String> = BTreeMap::new();
    let mut applied = 0;
    let mut checked = 0;
    for (n, entry) in small.iter().step_by(stride).enumerate() {
        let Ok(state) = State::open(entry.bytes(), &Limits::default()) else {
            continue;
        };
        checked += 1;
        for k in 0..2u64 {
            let (id, seed) = (
                entry.id.clone(),
                0x9E37_79B9_7F4A_7C15u64.wrapping_mul(1 + k + n as u64),
            );
            let Ok(state) = State::open(entry.bytes(), &Limits::default()) else {
                continue;
            };
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                super::laws::sequence_on(state, &id, n as u64, seed, 2)
            }));
            match run {
                Ok(done) => applied += done,
                Err(payload) => {
                    let text = payload
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                        .unwrap_or_default();
                    failures.entry(entry.id.clone()).or_insert(text);
                }
            }
        }
        let _ = &state;
    }
    eprintln!(
        "{checked} corpus documents, {applied} batches applied, {} documents broke a law",
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} documents broke a law:\n{}",
        failures.len(),
        failures
            .iter()
            .map(|(id, why)| format!(
                "{id}: {}",
                why.lines().take(40).collect::<Vec<_>>().join(" | ")
            ))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
