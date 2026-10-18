//! The laws R1 to R11 over random batches of random documents, by several authors in sequence (design
//! `revisions.md`, 7.2 to 7.4). Each law is checked against something other than the code it checks: the
//! text model of `wordml` for text, the string model of `laws_gen` for what `Final` must read, `normalize`
//! for the XML, and the resolver itself for the laws that need accept and reject.
//!
//! This file makes the batches. A batch is of a kind (`Cfg` in `laws_checks.rs`): tracked or untracked, with
//! one compiler or several over one transaction, with accepts by id between the compilers. The laws that
//! look at the result are in `laws_checks.rs`, and each fails with the batch that broke it.

use super::compiler::Compiler;
use super::index::ChangeIndex;
use super::laws_checks::{Cfg, after_commit, base_of, before_commit, check_index, pick_accept};
use super::laws_gen::{Failure, law_docs, model_of, random_call, reconcile};
use super::laws_util::{census, dump};
use super::outcome::{Created, Effect, Outcome};
use super::resolve::resolve;
use super::stamp::{RevStamp, Track};
use super::tests_diff::Rng;
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::state::State;
use std::collections::HashSet;

/// What a batch leaves: the new state and, for the determinism law, its outcome as text.
pub(super) struct Step9 {
    pub state: State,
    pub outcome: String,
}

/// Runs one batch on a state. Returns the new state, or `None` if a call was refused. `doc` says which
/// document it is, for the message of a failure.
pub(super) fn step(state: &State, cfg: &Cfg, doc: &str) -> Option<Step9> {
    let stamp = RevStamp::new(&cfg.author, &cfg.date).unwrap();
    let track = if cfg.tracked {
        Track::Tracked(stamp)
    } else {
        Track::Untracked
    };
    let mut tx = state.begin();
    let mut model = model_of(&tx);
    let mut rng = Rng(cfg.seed | 1);
    let mut log: Vec<String> = Vec::new();
    let before_body = body_xml_of(state);
    let existing: HashSet<i64> = census(&state.parts()).iter().filter_map(|r| r.0).collect();
    let mut effects: Vec<Effect> = Vec::new();
    let mut created: Vec<Created> = Vec::new();
    let mut accepted: Vec<i64> = Vec::new();
    let mut index = ChangeIndex::new();
    let mut homes = {
        let view = tx.parts();
        let id = view.id(super::tests_support::MAIN).unwrap();
        super::laws_util::MarkerHomes::new(view.tree(id).unwrap())
    };
    let bug = |what: &str, log: &[String], body: &str| -> ! {
        panic!("{}", dump(&format!("{what}\n{cfg:?}"), doc, log, body))
    };
    if cfg.resolves
        && let Err(m) = check_index(&tx, &mut index)
    {
        bug(
            &format!("the index is wrong at the start: {m}"),
            &log,
            &body_xml(&tx),
        );
    }
    let n = 1 + rng.below(6);
    let mut done = 0;
    while done < n {
        let group = match cfg.per_compiler {
            0 => n - done,
            k => k.min(n - done),
        };
        {
            let mut c = Compiler::new(&mut tx, track.clone()).unwrap();
            for _ in 0..group {
                match random_call(&mut rng, &mut c, &mut model, &mut log) {
                    Ok(()) => {
                        if let Err(m) = reconcile(c.tx(), &mut model) {
                            let extra = format!(
                                "BEFORE: {}\nAFTER: {}",
                                body_xml_of(state),
                                body_xml(c.tx())
                            );
                            bug(&format!("R3 broken: {m}"), &log, &extra);
                        }
                        if let Some(fx) = c.effects_so_far().last().cloned() {
                            index.note(c.tx(), &fx);
                        }
                        note_markers(&mut homes, c.tx());
                    }
                    Err(Failure::Refused(_)) => {
                        // R9, atomicity: a refused batch leaves the state as it was.
                        assert_eq!(
                            body_xml_of(state),
                            before_body,
                            "a refused batch changed the state"
                        );
                        return None;
                    }
                    Err(Failure::Bug(m)) => {
                        bug(&format!("a call failed: {m}"), &log, &body_xml(c.tx()));
                    }
                }
                done += 1;
            }
            match c.finish() {
                Ok(o) => {
                    effects.extend(o.effects.iter().cloned());
                    // A compiler reports what it can see of what the transaction created, which is the
                    // touched blocks and its own new ones, so a transaction with several compilers has
                    // the union of their reports.
                    for item in o.created {
                        if !created
                            .iter()
                            .any(|c| (&c.part, c.node) == (&item.part, item.node))
                        {
                            created.push(item);
                        }
                    }
                }
                Err(e) => bug(&format!("finish failed: {e}"), &log, &body_xml(&tx)),
            }
        }
        if cfg.resolves {
            if let Err(m) = check_index(&tx, &mut index) {
                bug(&format!("R12 broken: {m}"), &log, &body_xml(&tx));
            }
            if done < n
                && let Some(id) =
                    pick_accept(&tx, &existing, &cut_so_far(&effects), &cfg.author, &mut rng)
            {
                log.push(format!("accept {id}"));
                let step = Step {
                    action: Action::Accept,
                    selector: Selector::Ids(vec![id]),
                };
                match resolve(&mut tx, &mut index, &[step]) {
                    Ok(res) => accepted.extend(
                        res.iter()
                            .flat_map(|r| r.resolved.iter().filter_map(|x| x.id)),
                    ),
                    Err(e) => bug(&format!("accepting {id} failed: {e}"), &log, &body_xml(&tx)),
                }
                note_markers(&mut homes, &tx);
                if let Err(m) = reconcile(&tx, &mut model) {
                    bug(
                        &format!("R3 broken by an accept: {m}"),
                        &log,
                        &body_xml(&tx),
                    );
                }
            }
        }
    }
    let outcome = Outcome {
        minted: tx.minted_annotation_ids(),
        effects,
        created,
        warnings: Vec::new(),
    };
    let xml = body_xml(&tx);
    let before_xml = body_xml_of(state);
    let fail = |m: &str| -> ! {
        bug(
            m,
            &log,
            &format!(
                "BEFORE: {before_xml}\nAFTER: {xml}\nEFFECTS: {:?}",
                outcome.effects
            ),
        )
    };
    let base_owned = base_of(state, &existing, &accepted).unwrap_or_else(|m| fail(&m));
    let base = base_owned.as_ref().unwrap_or(state);
    if let Err(m) = before_commit(state, base, &tx, &outcome, cfg, &homes) {
        fail(&m);
    }
    let (next, committed) = tx
        .commit()
        .unwrap_or_else(|e| bug(&format!("commit failed: {e}"), &log, &xml));
    if let Err(m) = after_commit(state, base, &next, &committed, &outcome, cfg) {
        fail(&m);
    }
    Some(Step9 {
        state: next,
        outcome: format!("{:?}", outcome),
    })
}

/// Notes where the markers that the batch made stand in the main part now (R6).
fn note_markers(homes: &mut super::laws_util::MarkerHomes, tx: &crate::state::Tx<'_>) {
    let view = tx.parts();
    if let Some(tree) = view
        .id(super::tests_support::MAIN)
        .and_then(|id| view.tree(id).ok())
    {
        homes.note(tree);
    }
}

/// The ids of the revisions that a batch has cut in two so far, by the old id.
fn cut_so_far(effects: &[Effect]) -> HashSet<i64> {
    effects
        .iter()
        .flat_map(|e| e.split.iter().flat_map(|(old, new)| [*old, *new]))
        .collect()
}

/// Runs one tracked batch with one compiler.
fn one_step(state: &State, author: &str, date: &str, seed: u64, doc: &str) -> Option<Step9> {
    step(state, &Cfg::plain(author, date, seed), doc)
}

const AUTHORS: [&str; 3] = ["Clauseo", "Zed", "Fay"];

/// A sequence of batches by different authors on one state, with the laws checked at every step. Each
/// batch is of a kind that the draw picks. Returns how many were applied.
pub(super) fn sequence_on(state: State, label: &str, salt: u64, seed: u64, steps: usize) -> usize {
    let mut rng = Rng(seed | 1);
    let mut state = state;
    let mut done = 0;
    for n in 0..steps {
        let author = AUTHORS[rng.below(AUTHORS.len())];
        let date = format!("2026-10-{:02}T10:00:00Z", 1 + n);
        let s = seed
            .wrapping_mul(31)
            .wrapping_add((n as u64) << 8)
            .wrapping_add(salt);
        // A refused call drops the batch: the next attempt draws another.
        for attempt in 0..4u64 {
            let cfg = Cfg::drawn(&mut rng, author, &date, s.wrapping_add(attempt * 7919));
            if let Some(next) = step(&state, &cfg, label) {
                state = next.state;
                done += 1;
                break;
            }
        }
    }
    done
}

/// Runs `seeds` sequences of four batches on every document and checks that most were applied.
fn soak(seeds: u64) {
    let docs = law_docs();
    let mut applied = 0;
    let mut total = 0;
    for (i, doc) in docs.iter().enumerate() {
        for k in 0..seeds {
            applied += sequence_on(
                state(doc),
                &format!("{doc}\n(document {i}, sequence {k})"),
                i as u64,
                0x2545_F491_4F6C_DD1Du64.wrapping_mul(1 + i as u64 * 1000 + k),
                4,
            );
            total += 4;
        }
    }
    eprintln!("batches applied {applied} of {total}");
    assert!(
        applied * 3 > total,
        "too many refused batches: {applied} of {total}"
    );
}

/// The default run: 12 sequences on each document, every pull request.
#[test]
fn the_laws_hold_for_random_batches_by_several_authors_on_every_document() {
    soak(12);
}

/// The long run, for a change to the compiler or the resolver: `cargo test -p stetdoc-core -- --ignored
/// soak`. 400 sequences on each document, about two minutes.
#[test]
#[ignore = "a long run, started by hand"]
fn soak_the_laws_on_four_hundred_sequences_per_document() {
    soak(400);
}

#[test]
fn the_same_bytes_batch_and_stamp_give_the_same_output() {
    // R9: determinism, for every kind of batch.
    let docs = law_docs();
    let kinds = |seed: u64| -> Vec<Cfg> {
        let plain = Cfg::plain("Clauseo", "2026-10-05T14:03:27Z", seed);
        vec![
            plain.clone(),
            Cfg {
                per_compiler: 2,
                ..plain.clone()
            },
            Cfg {
                per_compiler: 1,
                resolves: true,
                ..plain.clone()
            },
            Cfg {
                tracked: false,
                ..plain
            },
        ]
    };
    for (i, doc) in docs.iter().enumerate() {
        let s = state(doc);
        for k in 0..6u64 {
            let seed = 0x9E37_79B9_7F4A_7C15u64.wrapping_mul(1 + k + i as u64 * 10);
            for cfg in kinds(seed) {
                let a = step(&s, &cfg, doc);
                let b = step(&s, &cfg, doc);
                assert_eq!(a.is_some(), b.is_some());
                if let (Some(a), Some(b)) = (a, b) {
                    assert_eq!(
                        body_xml_of(&a.state),
                        body_xml_of(&b.state),
                        "doc {i} seed {seed} {cfg:?}"
                    );
                    assert_eq!(a.outcome, b.outcome, "doc {i} seed {seed} {cfg:?}");
                }
            }
        }
    }
}
