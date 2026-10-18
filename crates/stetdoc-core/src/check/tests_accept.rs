//! V4 of the design (`docs/design/check.md` 9) on the corpus: an edit that adds nothing is accepted,
//! on every document that opens, whether or not it has problems already. For a sample of the
//! paragraphs of each story part, those that have a finding first, four edits are made through the
//! `Editor` on a `Tx`: the text of a run is replaced, a run is inserted where the schema puts it,
//! a paragraph is inserted next to it, and an attribute is set. None of them can add a key, so a
//! refusal is a bug of `check`, of the units it finds, or of the edit.
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR`, and the test skips when it is
//! unset. `cargo xtask check` always sets it.

use super::document::{SchemaPart, listing};
use super::tests::W;
use super::units::{Layout, layout_of};
use super::{Declared, edit, scan_part};
use crate::api::limits::Limits;
use crate::schema::{
    Fit, PartKind, Prefer, ctx_at, insert_in_order, is_unit_boundary, unit_root, validate_part,
};
use crate::state::State;
use crate::xml::{NodeId, SpanTree};
use stetdoc_testkit::corpus::Corpus;

/// The paragraphs to edit in a part: the first with a finding, then spread through the part.
fn sample(kind: PartKind, tree: &SpanTree) -> Vec<NodeId> {
    let mut found: Vec<NodeId> = validate_part(kind, tree)
        .violations
        .iter()
        .map(|v| v.node)
        .chain(scan_part(tree).iter().filter_map(|f| f.node))
        .map(|n| unit_root(tree, n))
        .filter(|n| is_unit_boundary(tree, *n) && tree.node_ref(*n).local_name() == "p")
        .collect();
    found.dedup();
    found.truncate(2);
    let all = super::tests::named(tree, "p");
    let step = (all.len() / 2).max(1);
    for p in all.into_iter().step_by(step).take(2) {
        if !found.contains(&p) {
            found.push(p);
        }
    }
    found
}

/// The first `w:t` text node of a paragraph, outside any paragraph below it.
fn text_node(tree: &SpanTree, p: NodeId) -> Option<NodeId> {
    let mut stack = vec![p];
    while let Some(id) = stack.pop() {
        let el = tree.node_ref(id);
        if el.local_name() == "t" {
            return el.first_child().map(|c| c.id());
        }
        if id != p && is_unit_boundary(tree, id) {
            continue;
        }
        let mark = stack.len();
        stack.extend(el.child_elements().map(|c| c.id()));
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    None
}

/// Makes the edit of this kind, and says whether it could be made.
fn make(kind: PartKind, tree: &mut SpanTree, p: NodeId, which: usize) -> bool {
    match which {
        0 => match text_node(tree, p) {
            Some(text) => tree.edit().set_text(text, "edited").is_ok(),
            None => false,
        },
        1 => {
            let ctx = ctx_at(kind, tree, p);
            let placement = insert_in_order(ctx, tree, p, (W, "r"), Prefer::Last);
            if !matches!(placement.fit, Fit::Valid) {
                return false;
            }
            let kids: Vec<NodeId> = tree.node_ref(p).child_elements().map(|c| c.id()).collect();
            let at = kids.get(placement.index).copied();
            let (r, t) = (create(tree, "r"), create(tree, "t"));
            let Ok(text) = tree.edit().create_text("inserted") else {
                return false;
            };
            let mut ed = tree.edit();
            ed.append_child(t, text).is_ok()
                && ed.append_child(r, t).is_ok()
                && ed.insert_before(p, at, r).is_ok()
        }
        2 => {
            // a paragraph after the last paragraph of a comment would make the one the comment's
            // extension rows name no longer the last, which is an edit that adds a problem: so in
            // the comments part the paragraph goes before
            let (Some(parent), next) = (
                tree.node_ref(p).parent().map(|x| x.id()),
                if kind == PartKind::Comments {
                    Some(p)
                } else {
                    tree.node_ref(p).next_sibling().map(|x| x.id())
                },
            ) else {
                return false;
            };
            let (new, r, t) = (create(tree, "p"), create(tree, "r"), create(tree, "t"));
            let Ok(text) = tree.edit().create_text("another") else {
                return false;
            };
            let mut ed = tree.edit();
            ed.append_child(t, text).is_ok()
                && ed.append_child(r, t).is_ok()
                && ed.append_child(new, r).is_ok()
                && ed.insert_before(parent, next, new).is_ok()
        }
        _ => tree.edit().set_attr(p, W, "rsidR", "00AB12CD").is_ok(),
    }
}

fn create(tree: &mut SpanTree, local: &str) -> NodeId {
    tree.edit().create_element(W, local).unwrap_or(p_of(tree))
}

/// Never reached: creating an element in a tree that has parsed does not fail.
fn p_of(tree: &SpanTree) -> NodeId {
    tree.root().id()
}

#[test]
fn v4_edits_that_add_nothing_are_accepted_on_every_document_that_opens() {
    let Some(corpus) = Corpus::from_env() else {
        eprintln!("STETDOC_CORPUS_DIR is not set: the corpus is not read");
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut tally = Tally::default();
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let state = State::open(entry.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open and does not: {e}", entry.id));
        let view = state.parts();
        let Ok(list) = listing(&view) else { continue };
        for part in list
            .parts
            .iter()
            .filter(|p| layout_of(p.kind) != Layout::Elements)
        {
            let Ok(tree) = view.tree(part.id) else {
                continue;
            };
            for (i, p) in sample(part.kind, tree).into_iter().enumerate() {
                for which in [i % 4, (i + 1) % 4] {
                    judge(&state, part, p, which, &entry.id, &mut tally);
                }
            }
        }
    }
    println!(
        "edits made {}, refused {}, with a problem carried {}, that validated nothing {}",
        tally.made,
        tally.refused.len(),
        tally.carried,
        tally.unseen.len()
    );
    assert!(tally.made > 500, "{} edits", tally.made);
    assert!(
        tally.refused.is_empty(),
        "{} edits that add nothing were refused:\n{:#?}",
        tally.refused.len(),
        &tally.refused[..tally.refused.len().min(12)]
    );
    assert!(
        tally.unseen.is_empty(),
        "edits that no unit was validated for: {:#?}",
        &tally.unseen[..tally.unseen.len().min(12)]
    );
    assert!(
        tally.carried > 20,
        "only {} edits met a problem that was already there",
        tally.carried
    );
}

/// What the edits came to.
#[derive(Default)]
struct Tally {
    made: usize,
    carried: usize,
    refused: Vec<String>,
    unseen: Vec<String>,
}

fn judge(state: &State, part: &SchemaPart, p: NodeId, which: usize, id: &str, tally: &mut Tally) {
    let mut tx = state.begin();
    let Ok(tree) = tx.part_mut(&part.name) else {
        return;
    };
    if !make(part.kind, tree, p, which) {
        return;
    }
    tally.made += 1;
    let at = format!("{id}: {} edit {which} at {p:?}", part.name);
    match edit(&tx, &Declared::default()) {
        Ok(v) => {
            tally.carried += usize::from(v.carried_total > 0);
            if v.checked.validated == 0 {
                tally.unseen.push(at);
            }
        }
        Err(e) => tally.refused.push(format!("{at}: {}", e.message())),
    }
}
