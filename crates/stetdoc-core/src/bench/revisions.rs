//! `revisions_*`: what the compiler and the resolver cost on the shared inputs. A batch of fifty tracked
//! replacements through one compiler, with the commit and without (`revisions_edit_50`,
//! `revisions_compile_50`), accepting everything (`revisions_accept_all`), and the three shapes of a
//! conversation that resolves as it goes on a document that holds revisions: twenty accepts and rejects by id
//! one after the other (`revisions_resolve_ids_20`), ten rounds of resolving one change and editing
//! (`revisions_resolve_then_edit_10`), and the changes of twenty single blocks, which is what a receipt asks
//! for (`revisions_changes_receipt_20`).
//!
//! The replacements are in fifty paragraphs spread through the main part, one word in each, in paragraphs
//! that hold no field (see `holds_field`). The documents that hold revisions are made by the same
//! replacements, by another author, when a benchmark is prepared, so the clock does not see them. The three
//! shapes that resolve are held to a time of their own: what they cost has to follow the blocks they name and
//! not the size of the document, so one figure is right for every input. What is left in them that grows with
//! the document is the refresh of the story after each write, which `state` and `wordml` own.

use super::support::{holds_field, spread};
use crate::api::limits::Limits;
use crate::revisions::changes::changes;
use crate::revisions::compiler::Compiler;
use crate::revisions::index::ChangeIndex;
use crate::revisions::outcome::Outcome;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::{RevStamp, Track};
use crate::revisions::types::{Action, Selector, Step};
use crate::state::{State, Tx};
use crate::wordml::{BlockKey, BlockKind, Mode, part_revisions};
use std::ops::Range;
use std::time::Duration;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

const MAIN: &str = "word/document.xml";

/// How many operations a batch has: the budget of an edit is for fifty.
const OPERATIONS: usize = 50;

/// How many replacements the documents that hold revisions are made with.
const REVISED: usize = 40;

/// Fifty tracked replacements through one compiler, without the commit.
pub(super) const COMPILE: Bench = Bench {
    name: "revisions_compile_50",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: compile,
};

/// The same, and the commit: a fifty-operation edit.
pub(super) const EDIT: Bench = Bench {
    name: "revisions_edit_50",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: edit,
};

/// Accepting every revision of a document that holds some, and the commit.
pub(super) const ACCEPT_ALL: Bench = Bench {
    name: "revisions_accept_all",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: accept_all,
};

/// Twenty accepts and rejects by id, one step each, in one transaction.
pub(super) const RESOLVE_IDS: Bench = Bench {
    name: "revisions_resolve_ids_20",
    limit: Limit::Time(Duration::from_millis(150)),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: resolve_ids,
};

/// Ten rounds of resolving one change by id and then editing.
pub(super) const RESOLVE_THEN_EDIT: Bench = Bench {
    name: "revisions_resolve_then_edit_10",
    limit: Limit::Time(Duration::from_millis(150)),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: resolve_then_edit,
};

/// The changes of twenty single blocks, as twenty receipts ask for them.
pub(super) const CHANGES_RECEIPT: Bench = Bench {
    name: "revisions_changes_receipt_20",
    limit: Limit::Time(Duration::from_millis(60)),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: changes_receipt,
};

fn open(input: &Input) -> Result<State, String> {
    State::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())
}

fn stamp(author: &str) -> Result<RevStamp, String> {
    RevStamp::new(author, "2026-02-01T00:00:00Z").map_err(|e| e.to_string())
}

/// The first word of at least two characters of a text, as a range of characters.
fn first_word(text: &str) -> Option<Range<usize>> {
    let mut start = None;
    for (i, c) in text.chars().enumerate() {
        match (start, c.is_alphanumeric()) {
            (None, true) => start = Some(i),
            (Some(s), false) if i - s >= 2 => return Some(s..i),
            (Some(_), false) => start = None,
            _ => {}
        }
    }
    let end = text.chars().count();
    start.filter(|s| end - s >= 2).map(|s| s..end)
}

/// A paragraph to edit: its key, the word to replace, and its place among the blocks of the story.
type Pick = (BlockKey, Range<usize>, usize);

/// `count` paragraphs spread through the main part, each with a word to replace.
fn picks(state: &State, count: usize) -> Result<Vec<Pick>, String> {
    let view = state.parts();
    let id = view.id(MAIN).ok_or("the document has no main part")?;
    let story = view.story(id).map_err(|e| e.to_string())?;
    let tree = view.tree(id).map_err(|e| e.to_string())?;
    let mut usable: Vec<Pick> = Vec::new();
    for (i, e) in story.blocks().iter().enumerate() {
        if e.kind != BlockKind::Paragraph {
            continue;
        }
        let Some(p) = story.paragraph(tree, i) else {
            continue;
        };
        if holds_field(tree, e.node) {
            continue;
        }
        if let Some(range) = first_word(&p.text(Mode::Final)) {
            usable.push((story.key(e), range, i));
        }
    }
    if usable.is_empty() {
        return Err("the body has no paragraph with a word in it".to_owned());
    }
    Ok(spread(&usable, count))
}

/// Makes the replacements in a transaction, by one author.
fn replace_all(tx: &mut Tx<'_>, picks: &[Pick], author: &str) -> Result<Outcome, String> {
    let mut compiler =
        Compiler::new(tx, Track::Tracked(stamp(author)?)).map_err(|e| e.to_string())?;
    for (key, range, _) in picks {
        compiler
            .replace_text(key, range.clone(), "REVISED")
            .map_err(|e| e.to_string())?;
    }
    compiler.finish().map_err(|e| e.to_string())
}

fn compile(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    let picks = picks(&state, OPERATIONS)?;
    Ok(Box::new(move || {
        let mut tx = state.begin();
        let outcome = replace_all(&mut tx, &picks, "Bench")?;
        drop(tx);
        Ok(Box::new((state, outcome)) as Kept)
    }))
}

fn edit(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    let picks = picks(&state, OPERATIONS)?;
    Ok(Box::new(move || {
        let mut tx = state.begin();
        let outcome = replace_all(&mut tx, &picks, "Bench")?;
        let committed = tx.commit().map_err(|e| e.to_string())?;
        Ok(Box::new((state, outcome, committed)) as Kept)
    }))
}

/// A document that holds revisions, which the replacements of another author made, and where they are.
fn revised(input: &Input) -> Result<(State, Vec<Pick>), String> {
    let state = open(input)?;
    let picks = picks(&state, REVISED)?;
    let mut tx = state.begin();
    replace_all(&mut tx, &picks, "Eric")?;
    let (next, _) = tx.commit().map_err(|e| e.to_string())?;
    Ok((next, picks))
}

fn step(action: Action, selector: Selector) -> Step {
    Step { action, selector }
}

fn accept_all(input: &Input) -> Result<Run, String> {
    let (state, _) = revised(input)?;
    Ok(Box::new(move || {
        let mut tx = state.begin();
        let resolved = resolve(
            &mut tx,
            &mut ChangeIndex::new(),
            &[step(Action::Accept, Selector::All)],
        )
        .map_err(|e| e.to_string())?;
        let committed = tx.commit().map_err(|e| e.to_string())?;
        Ok(Box::new((state, resolved, committed)) as Kept)
    }))
}

/// `count` ids of revisions spread through the main part of a state.
fn ids(state: &State, count: usize) -> Result<Vec<i64>, String> {
    let view = state.parts();
    let id = view.id(MAIN).ok_or("the document has no main part")?;
    let tree = view.tree(id).map_err(|e| e.to_string())?;
    let all: Vec<i64> = part_revisions(tree)
        .into_iter()
        .filter_map(|r| r.id)
        .collect();
    if all.is_empty() {
        return Err("the document holds no revision".to_owned());
    }
    Ok(spread(&all, count))
}

fn resolve_ids(input: &Input) -> Result<Run, String> {
    let (state, _) = revised(input)?;
    let ids = ids(&state, 20)?;
    Ok(Box::new(move || {
        let mut tx = state.begin();
        let mut index = ChangeIndex::new();
        let mut results = Vec::new();
        for (k, id) in ids.iter().enumerate() {
            let action = if k % 2 == 0 {
                Action::Accept
            } else {
                Action::Reject
            };
            // An id that an earlier step resolved with its change is gone, and that is not a failure.
            if let Ok(r) = resolve(
                &mut tx,
                &mut index,
                &[step(action, Selector::Ids(vec![*id]))],
            ) {
                results.push(r);
            }
        }
        drop(tx);
        Ok(Box::new((state, results)) as Kept)
    }))
}

fn resolve_then_edit(input: &Input) -> Result<Run, String> {
    let (state, picks) = revised(input)?;
    let ids = ids(&state, 10)?;
    Ok(Box::new(move || {
        let mut tx = state.begin();
        let mut index = ChangeIndex::new();
        let mut kept = Vec::new();
        for (k, id) in ids.iter().enumerate() {
            if let Ok(r) = resolve(
                &mut tx,
                &mut index,
                &[step(Action::Accept, Selector::Ids(vec![*id]))],
            ) {
                kept.push(r);
            }
            let mut compiler = Compiler::new(&mut tx, Track::Tracked(stamp("Bench")?))
                .map_err(|e| e.to_string())?;
            // A paragraph that the resolution joined or removed is skipped, as a batch would.
            let Some((key, range, _)) = picks.get(k * 3) else {
                continue;
            };
            if let Ok(fx) = compiler.replace_text(key, range.clone(), "REVISED") {
                index.note(compiler.tx(), &fx);
            }
            compiler.finish().map_err(|e| e.to_string())?;
        }
        drop(tx);
        Ok(Box::new((state, kept)) as Kept)
    }))
}

fn changes_receipt(input: &Input) -> Result<Run, String> {
    let (state, picks) = revised(input)?;
    let blocks: Vec<usize> = spread(&picks, 20).into_iter().map(|(_, _, i)| i).collect();
    Ok(Box::new(move || {
        let view = state.parts();
        let id = view.id(MAIN).ok_or("the document has no main part")?;
        let story = view.story(id).map_err(|e| e.to_string())?;
        let tree = view.tree(id).map_err(|e| e.to_string())?;
        let found: Vec<_> = blocks
            .iter()
            .map(|&n| changes(&story, tree, Some(n..n + 1)))
            .collect();
        Ok(Box::new((state, found)) as Kept)
    }))
}
