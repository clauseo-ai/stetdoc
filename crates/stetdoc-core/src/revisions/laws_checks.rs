//! The laws that are checked on a batch (design `revisions.md`, 7.2 and 7.3), each as a function from what
//! the batch did to what is wrong, if anything. `laws.rs` makes the batches and calls these.
//!
//! A batch is of a kind (`Cfg`): tracked or untracked, with one compiler or several over the one
//! transaction, with accepts by id between the compilers. R1, R2 and R4 are laws of tracked batches. A
//! batch that accepts changes is compared with `base`, the state before it with the same accepts made
//! alone, because the accepts change what `Original` reads and the laws compare with what it should be.

use super::changes::{ChangeKind, changes};
use super::index::ChangeIndex;
use super::laws_refs::{Reasons, check_refs};
use super::laws_select::{check_ids, check_windows};
use super::laws_util::{
    Census, after, after_with, census, ends_in_an_inserted_mark, final_runs, first_difference,
    normalized, original_runs_in, skeleton,
};
use super::outcome::Outcome;
use super::resolve::resolve;
use super::tests_diff::Rng;
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::state::{Committed, State, Tx};
use crate::wordml::{BlockKey, Mode, part_revisions};
use std::collections::HashSet;

/// What kind of batch a step makes.
#[derive(Clone, Debug)]
pub(super) struct Cfg {
    pub author: String,
    pub date: String,
    pub seed: u64,
    /// Tracked, or untracked.
    pub tracked: bool,
    /// How many calls one compiler makes before another takes the transaction: 0 for one compiler.
    pub per_compiler: usize,
    /// Accept a change by id between the compilers.
    pub resolves: bool,
}

impl Cfg {
    /// A tracked batch with one compiler.
    pub(super) fn plain(author: &str, date: &str, seed: u64) -> Cfg {
        Cfg {
            author: author.to_owned(),
            date: date.to_owned(),
            seed,
            tracked: true,
            per_compiler: 0,
            resolves: false,
        }
    }

    /// The kind of batch that a draw picks: most are plain.
    pub(super) fn drawn(rng: &mut Rng, author: &str, date: &str, seed: u64) -> Cfg {
        let plain = Cfg::plain(author, date, seed);
        match rng.below(10) {
            0..=4 => plain,
            5 | 6 => Cfg {
                per_compiler: 1 + rng.below(3),
                ..plain
            },
            7 | 8 => Cfg {
                per_compiler: 1 + rng.below(2),
                resolves: true,
                ..plain
            },
            _ => Cfg {
                tracked: false,
                ..plain
            },
        }
    }
}

/// The changes that an accept may take in the middle of a batch without changing what `Final` reads or
/// which paragraphs there are: no mark, row or move.
fn is_safe_to_accept(kind: ChangeKind) -> bool {
    matches!(
        kind,
        ChangeKind::Insertion
            | ChangeKind::Deletion
            | ChangeKind::RunFormat
            | ChangeKind::ParagraphFormat
    )
}

/// Picks a change of another author that was in the document before the batch, whole, and is safe to
/// accept, and returns the first id of it. The author's own pending changes are not picked, because a batch
/// may take text out of them outright, and nor are changes that the batch cut in two (`cut`): what is left of
/// either when it is accepted is not what accepting it alone on the state before would leave.
pub(super) fn pick_accept(
    tx: &Tx<'_>,
    existing: &HashSet<i64>,
    cut: &HashSet<i64>,
    author: &str,
    rng: &mut Rng,
) -> Option<i64> {
    let view = tx.parts();
    let id = view.id(MAIN)?;
    let (story, tree) = (view.story(id).ok()?, view.tree(id).ok()?);
    let safe: Vec<i64> = changes(&story, tree, None)
        .into_iter()
        .filter(|c| c.author.as_deref() != Some(author) && !c.ids.iter().any(|i| cut.contains(i)))
        .filter(|c| is_safe_to_accept(c.kind) && c.ids.iter().all(|i| existing.contains(i)))
        .filter_map(|c| c.ids.first().copied())
        .collect();
    safe.get(rng.below(safe.len().max(1))).copied()
}

/// The state before a batch with the elements that the batch accepted accepted alone, those of them that
/// were there. Which elements one change takes depends on the paragraphs around it, so the elements that
/// the batch resolved are named, and not the ids it was asked about.
pub(super) fn base_of(
    state: &State,
    existing: &HashSet<i64>,
    accepted: &[i64],
) -> Result<Option<State>, String> {
    let ids: Vec<i64> = accepted
        .iter()
        .copied()
        .filter(|i| existing.contains(i))
        .collect();
    if ids.is_empty() {
        return Ok(None);
    }
    let mut tx = state.begin();
    let step = Step {
        action: Action::Accept,
        selector: Selector::Elements(ids.clone()),
    };
    resolve(&mut tx, &mut ChangeIndex::new(), &[step]).map_err(|e| {
        format!("accepting {ids:?} alone on the state the batch began on failed: {e}")
    })?;
    let (next, _) = tx
        .commit()
        .map_err(|e| format!("the accepts alone did not commit: {e}"))?;
    Ok(Some(next))
}

/// The index agrees with the document: every revision element with an id is found by it.
pub(super) fn check_index(tx: &Tx<'_>, index: &mut ChangeIndex) -> Result<(), String> {
    let view = tx.parts();
    let id = view.id(MAIN).ok_or("the document has no main part")?;
    let tree = view.tree(id).map_err(|e| e.to_string())?;
    for r in part_revisions(tree) {
        let Some(rid) = r.id else { continue };
        let places = index.lookup(tx, rid).map_err(|e| e.to_string())?;
        if !places
            .iter()
            .any(|(part, node)| &**part == MAIN && *node == r.node)
        {
            return Err(format!(
                "the index does not find revision {rid} ({:?}), which is in the document",
                r.kind
            ));
        }
    }
    Ok(())
}

/// The laws that look at the transaction before it commits: R1 (the text half), R5 and R6.
pub(super) fn before_commit(
    before: &State,
    base: &State,
    tx: &Tx<'_>,
    outcome: &Outcome,
    cfg: &Cfg,
    homes: &super::laws_util::MarkerHomes,
) -> Result<(), String> {
    // R1, the text half: the Original join runs are those the batch started with.
    if cfg.tracked {
        let (runs, want) = (
            run_texts(&tx.parts(), Mode::Original),
            run_texts(&base.parts(), Mode::Original),
        );
        if runs != want {
            return Err(format!(
                "R1 broken: the Original runs changed\n before {want:?}\n after  {runs:?}"
            ));
        }
    }
    // R5: no new schema violations (the Word rules were checked by `finish`).
    let added = schema_added(before, tx);
    if !added.is_empty() {
        return Err(format!("R5 broken: new schema violations {added:?}"));
    }
    // R6: outside the blocks that were touched or made, the bytes of the part are those it had.
    let view = tx.parts();
    let id = view.id(MAIN).ok_or("no main part")?;
    let tree = view.tree(id).map_err(|e| e.to_string())?;
    let old_tree = before.parts().tree(id).map_err(|e| e.to_string())?;
    let touched: HashSet<_> = tx.touched().iter().map(|k| k.node()).collect();
    let new: HashSet<_> = outcome
        .effects
        .iter()
        .flat_map(|e| e.new_blocks.iter().map(|k| k.node()))
        .collect();
    // A block that is no longer in the part was removed on purpose: it is left out of the old dump
    // whole, touched or not (a removal is not a touch).
    let old_story = before.parts().story(id).map_err(|e| e.to_string())?;
    let removed: HashSet<_> = old_story
        .blocks()
        .iter()
        .map(|e| e.node)
        .filter(|n| !super::cleanup::is_attached(tree, *n))
        .collect();
    let mut new = new;
    new.extend(super::laws_util::hoisted_markers(
        old_tree, tree, &removed, &touched, &new, homes,
    ));
    let (a, b) = (
        skeleton(old_tree, &touched, &removed),
        skeleton(tree, &touched, &new),
    );
    if a != b {
        return Err(format!(
            "R6 broken: something outside the touched blocks changed\n{}",
            first_difference(&a, &b)
        ));
    }
    Ok(())
}

/// Every revision element of the result is in exactly one change.
fn partition(next: &State) -> Result<(), String> {
    let view = next.parts();
    let id = view.id(MAIN).ok_or("no main part")?;
    let tree = view.tree(id).map_err(|e| e.to_string())?;
    let story = view.story(id).map_err(|e| e.to_string())?;
    let cs = changes(&story, tree, None);
    let mut nodes: Vec<String> = cs
        .iter()
        .flat_map(|c| c.nodes.iter().map(|n| format!("{n:?}")))
        .collect();
    let listed = nodes.len();
    nodes.sort();
    nodes.dedup();
    let total = part_revisions(tree).len();
    if nodes.len() != listed || listed != total {
        return Err(format!(
            "changes() broken: {total} revision elements, {listed} in changes, {} distinct",
            nodes.len()
        ));
    }
    Ok(())
}

/// The pairs that a batch's merges declared, as the block that was absorbed and the block that remains. A
/// call that merges says what each merge made in its origins, and a deletion of a table makes several: the
/// block of an origin is what remains of the blocks it came from.
fn merged_of(outcome: &Outcome) -> Vec<(BlockKey, BlockKey)> {
    outcome
        .effects
        .iter()
        .filter_map(|e| {
            let survivor = e.survivor.as_ref()?;
            e.origins.iter().find(|o| o.block == *survivor)
        })
        .flat_map(|o| {
            o.from
                .iter()
                .filter(move |k| **k != o.block)
                .map(move |k| (k.clone(), o.block.clone()))
        })
        .collect()
}

/// R4: a revision that was there is as it was, unless the outcome says otherwise.
pub(super) fn existing_revisions_are_untouched(
    base: &State,
    next: &State,
    outcome: &Outcome,
) -> Result<(), String> {
    let (before, after) = (census(&base.parts()), census(&next.parts()));
    let mut allowed: HashSet<i64> = HashSet::new();
    for e in &outcome.effects {
        allowed.extend(e.removed_own.iter().filter_map(|r| r.id));
        allowed.extend(e.swallowed.iter().copied());
        allowed.extend(e.folded.iter().map(|f| f.id));
    }
    // What a revision holds changes only when it was cut in two, which the outcome names by the old id.
    let cut: HashSet<i64> = outcome
        .effects
        .iter()
        .flat_map(|e| e.split.iter().map(|(old, _)| *old))
        .collect();
    for rev in &before {
        let Some(id) = rev.0 else { continue };
        let found = after.iter().filter(|r| r.0 == Some(id)).any(|r| {
            (r.0, r.1, &r.2, &r.3) == (rev.0, rev.1, &rev.2, &rev.3)
                && (r.4 == rev.4 || cut.contains(&id))
        });
        if !found && !allowed.contains(&id) {
            return Err(format!(
                "R4 broken: the revision {rev:?} changed or vanished and the outcome does not say so"
            ));
        }
    }
    Ok(())
}

/// R13: every block a call made declares where it came from (an empty list says it was made from nothing), and
/// what `Outcome::origins` hands on is made of blocks that the document had before the batch, which is what
/// `check` can read an origin from.
pub(super) fn origins_are_declared(before: &State, outcome: &Outcome) -> Result<(), String> {
    for (i, e) in outcome.effects.iter().enumerate() {
        for new in &e.new_blocks {
            if !e.origins.iter().any(|o| o.block == *new) {
                return Err(format!(
                    "R13 broken: call {i} made the block {:?} and declared no origin for it",
                    new.node()
                ));
            }
        }
    }
    let view = before.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let base: HashSet<BlockKey> = story.blocks().iter().map(|e| story.key(e)).collect();
    for o in outcome.origins() {
        for from in &o.from {
            if !base.contains(from) {
                return Err(format!(
                    "R13 broken: the origin of {:?} names {:?}, which the document did not have before the batch",
                    o.block.node(),
                    from.node()
                ));
            }
        }
    }
    Ok(())
}

/// R2: rejecting what was written gives the original, up to what the outcome carves out of the author's own
/// revisions: own text removed outright, own changes folded into, and own revisions cut in two. What the
/// batch cut of another author's revisions is not carved out: rejecting the author's work leaves it as it
/// was, up to the merging of the pieces, which `normalize` does.
fn rejecting_the_work_gives_the_original(
    base: &State,
    next: &State,
    outcome: &Outcome,
    cfg: &Cfg,
) -> Result<(), String> {
    let (before, now) = (census(&base.parts()), census(&next.parts()));
    let exists = |c: &Census, id: i64| c.iter().any(|r| r.0 == Some(id));
    // An id is the author's if the revision with it is by her, or, when it is gone, if it is a piece of one
    // that was (the copy of another author's revision is that author's), or if the transaction minted it for
    // something of her own.
    let copy_of: std::collections::HashMap<i64, i64> = outcome
        .effects
        .iter()
        .flat_map(|e| e.split.iter().map(|(old, new)| (*new, *old)))
        .collect();
    let is_own = |id: i64| {
        let mut cur = id;
        for _ in 0..=copy_of.len() {
            if let Some(r) = now.iter().chain(before.iter()).find(|r| r.0 == Some(cur)) {
                return r.2.as_deref() == Some(cfg.author.as_str());
            }
            match copy_of.get(&cur) {
                Some(&old) => cur = old,
                None => break,
            }
        }
        u32::try_from(cur).is_ok_and(|i| outcome.minted.contains(&i))
    };
    let mut carved: HashSet<i64> = HashSet::new();
    for e in &outcome.effects {
        carved.extend(e.removed_own.iter().filter_map(|r| r.id));
        carved.extend(e.folded.iter().map(|f| f.id));
        for (old, new) in &e.split {
            if is_own(*old) {
                carved.insert(*old);
                carved.insert(*new);
            }
        }
    }
    // An id that a batch frees before it takes its first one is taken again, so the same number can name
    // an element of the author's in one document and another author's in the other. Only the author's own
    // are rejected on each side.
    let own_in = |c: &Census, id: i64| {
        c.iter()
            .any(|r| r.0 == Some(id) && r.2.as_deref() == Some(cfg.author.as_str()))
    };
    let mut ids_n: Vec<i64> = outcome.created.iter().map(|c| c.id).collect();
    ids_n.extend(carved.iter().copied());
    ids_n.retain(|i| own_in(&now, *i));
    ids_n.sort_unstable();
    ids_n.dedup();
    let swallowed: Vec<i64> = outcome
        .effects
        .iter()
        .flat_map(|e| e.swallowed.iter().copied())
        .filter(|i| exists(&before, *i))
        .collect();
    let mut ids_d: Vec<i64> = carved
        .iter()
        .copied()
        .filter(|i| own_in(&before, *i) && !swallowed.contains(i))
        .collect();
    ids_d.sort_unstable();
    let step = |ids: Vec<i64>| -> Vec<Step> {
        if ids.is_empty() {
            Vec::new()
        } else {
            vec![Step {
                action: Action::Reject,
                selector: Selector::Elements(ids),
            }]
        }
    };
    let rn = after(next, &[], &step(ids_n.clone()));
    let rd = after(base, &swallowed, &step(ids_d.clone()));
    if rn != rd {
        return Err(format!(
            "R2 broken: rejecting {ids_n:?} of the result differs from rejecting {ids_d:?} of the original\n{}\nRESULT: {}\nORIGINAL: {}\nEFFECTS: {:#?}",
            first_difference(&rn, &rd),
            body_xml_of(next),
            body_xml_of(base),
            outcome.effects
        ));
    }
    Ok(())
}

/// R7 for the resolver: an accepted deletion is `deleted` and a rejected insertion is `rejected`.
fn refs_after_resolving(next: &State) -> Result<(), String> {
    for (action, reasons) in [
        (Action::Accept, Reasons::Accept),
        (Action::Reject, Reasons::Reject),
    ] {
        let mut tx = next.begin();
        let step = Step {
            action,
            selector: Selector::All,
        };
        let res = resolve(&mut tx, &mut ChangeIndex::new(), &[step])
            .map_err(|e| format!("R7: {action:?} all failed: {e}"))?;
        let joined: Vec<_> = res
            .iter()
            .flat_map(|r| r.joined.iter().map(|(a, b)| (b.clone(), a.clone())))
            .collect();
        let (resolved, committed) = tx
            .commit()
            .map_err(|e| format!("R7: commit after {action:?} all failed: {e}"))?;
        check_refs(next, &resolved, &committed, reasons, &joined)
            .map_err(|m| format!("R7 broken after {action:?} all: {m}"))?;
    }
    Ok(())
}

/// R8: accept all reads as `Final`, reject all as `Original`, nothing is left, and twice is the same.
fn resolving_everything(next: &State) -> Result<(), String> {
    let all = |action| {
        vec![Step {
            action,
            selector: Selector::All,
        }]
    };
    let mut tx = next.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &all(Action::Accept))
        .map_err(|e| format!("R8: accept all failed: {e}"))?;
    if final_runs(&tx.parts()) != final_runs(&next.parts()) {
        return Err(format!(
            "R8 broken: accept all does not read as Final\n final {:?}\n accepted {:?}",
            final_runs(&next.parts()),
            final_runs(&tx.parts())
        ));
    }
    if !census(&tx.parts()).is_empty() {
        return Err("R8 broken: a revision is left after accept all".to_owned());
    }
    let added = schema_added(next, &tx);
    if !added.is_empty() {
        return Err(format!(
            "R5 broken: accept all adds schema violations {added:?}"
        ));
    }
    let once = normalized(&tx.parts());
    resolve(&mut tx, &mut ChangeIndex::new(), &all(Action::Accept))
        .map_err(|e| format!("R8: the second accept all failed: {e}"))?;
    if normalized(&tx.parts()) != once {
        return Err("R8 broken: accepting twice changed the document".to_owned());
    }
    let mut tx = next.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &all(Action::Reject))
        .map_err(|e| format!("R8: reject all failed: {e}"))?;
    let rejected = final_runs(&tx.parts());
    let original = original_runs_in(&next.parts(), MAIN);
    if rejected != original {
        return Err(format!(
            "R8 broken: reject all does not read as Original\n original {original:?}\n rejected {rejected:?}"
        ));
    }
    if !census(&tx.parts()).is_empty() {
        return Err("R8 broken: a revision is left after reject all".to_owned());
    }
    let added = schema_added(next, &tx);
    if !added.is_empty() {
        return Err(format!(
            "R5 broken: reject all adds schema violations {added:?}"
        ));
    }
    Ok(())
}

/// An untracked batch writes no revision: every revision of the result was there before, or is a piece of
/// one that the batch cut in two (a paragraph split cuts the wrapper that runs across it, and the new piece
/// takes a new id). It may take an annotation id for a bookmark or a comment range.
fn untracked_writes_no_revision(
    before: &State,
    next: &State,
    outcome: &Outcome,
) -> Result<(), String> {
    let mut known: HashSet<i64> = census(&before.parts()).iter().filter_map(|r| r.0).collect();
    known.extend(
        outcome
            .effects
            .iter()
            .flat_map(|e| e.split.iter().map(|(_, new)| *new)),
    );
    if let Some(r) = census(&next.parts())
        .iter()
        .find(|r| r.0.is_none_or(|i| !known.contains(&i)))
    {
        return Err(format!("an untracked batch wrote the revision {r:?}"));
    }
    Ok(())
}

/// The laws that look at the state a batch made, after the commit.
pub(super) fn after_commit(
    before: &State,
    base: &State,
    next: &State,
    committed: &Committed,
    outcome: &Outcome,
    cfg: &Cfg,
) -> Result<(), String> {
    partition(next)?;
    origins_are_declared(before, outcome)?;
    // R7: refs are right after the commit.
    let reasons = if cfg.tracked {
        Reasons::Batch
    } else {
        Reasons::Untracked
    };
    check_refs(before, next, committed, reasons, &merged_of(outcome))
        .map_err(|m| format!("R7 broken: {m}"))?;
    // R10 and R11: reading changes by window and by id agrees with reading the whole story.
    check_windows(next)?;
    check_ids(next)?;
    if cfg.tracked {
        existing_revisions_are_untouched(base, next, outcome)?;
        rejecting_the_work_gives_the_original(base, next, outcome, cfg)?;
        // R1, the XML half: rejecting everything gives the same document before and after.
        let all = vec![Step {
            action: Action::Reject,
            selector: Selector::All,
        }];
        let (ra, rb) = (after(next, &[], &all), after(base, &[], &all));
        if ra != rb {
            // A document that ends in a paragraph whose mark is an insertion has no defined original for
            // the properties of its last paragraph (see `normalize_without_last_ppr`).
            let tail = ends_in_an_inserted_mark(next) || ends_in_an_inserted_mark(base);
            let (ta, tb) = (
                after_with(next, &[], &all, true),
                after_with(base, &[], &all, true),
            );
            if !tail || ta != tb {
                return Err(format!(
                    "R1 broken: reject all differs\n{}",
                    first_difference(&ra, &rb)
                ));
            }
        }
    } else {
        untracked_writes_no_revision(before, next, outcome)?;
    }
    refs_after_resolving(next)?;
    resolving_everything(next)
}
