//! A document compared with itself, over the corpus named by `STETDOC_CORPUS_DIR`: every block of every story pairs with
//! itself, nothing is left over, and nothing is a move, a split or a merge. With the variable unset the test skips and
//! passes, as the other corpus tests of the core do.

use super::model::Atoms;
use super::stories::pair_documents;
use super::tables::StoryPairing;
use crate::api::limits::Limits;
use crate::state::State;
use stetdoc_testkit::corpus::Corpus;

/// True if every container pairing pairs each block with the block at the same index, and each paired table likewise.
fn identity(p: &StoryPairing, counts: &mut usize) -> bool {
    p.containers.iter().all(|c| {
        c.pairs.iter().all(|x| {
            *counts += 1;
            x.a == x.b
                && x.rows
                    .as_ref()
                    .is_none_or(|r| r.pairs.iter().all(|row| row.a == row.b))
        })
    })
}

#[test]
fn every_document_pairs_with_itself_and_leaves_nothing_over() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let mut checked = 0usize;
    let mut blocks = 0usize;
    for entry in corpus.expected_ok() {
        let bytes = entry.bytes();
        let Ok(a) = State::open(bytes.clone(), &Limits::default()) else {
            continue;
        };
        let Ok(b) = State::open(bytes, &Limits::default()) else {
            continue;
        };
        let Ok(pairings) = pair_documents(&a, &b, &mut Atoms::default()) else {
            continue;
        };
        for story in &pairings.stories {
            for root in &story.roots {
                assert!(
                    identity(&root.pairing, &mut blocks),
                    "{}: not the identity",
                    entry.id
                );
                assert!(
                    root.regrouped.moves.is_empty() && root.regrouped.groups.is_empty(),
                    "{}",
                    entry.id
                );
                // nothing is left over: every block of both containers is in a pair
                for c in &root.pairing.containers {
                    let la = story
                        .model_a
                        .containers
                        .get(c.ca)
                        .map_or(0, |k| k.blocks.len());
                    let lb = story
                        .model_b
                        .containers
                        .get(c.cb)
                        .map_or(0, |k| k.blocks.len());
                    assert_eq!(
                        (c.pairs.len(), c.pairs.len()),
                        (la, lb),
                        "{}: blocks left over",
                        entry.id
                    );
                }
            }
        }
        let plan = super::plan::plan(&pairings, &a, &b, &|x, y| x == y).expect("a plan");
        for sp in &plan.stories {
            assert!(
                sp.entries.is_empty(),
                "{}: a document differs from itself: {:?}",
                entry.id,
                sp.entries.first()
            );
        }
        assert_eq!(pairings.unmatched_slots, 0, "{}", entry.id);
        assert_eq!(pairings.table_replaced, 0, "{}", entry.id);
        checked += 1;
    }
    println!("compare self-pairing: {checked} documents, {blocks} pairs");
    assert!(checked > 100, "only {checked} documents were checked");
}

/// A copy of `state` with some paragraphs reworded and some deleted, untracked, so that every paragraph that survives
/// keeps its element and therefore its `NodeId`: the node ids are the truth that the pairing is read against.
fn edited(state: &State, every: usize) -> Option<State> {
    use crate::revisions::compiler::Compiler;
    use crate::revisions::stamp::Track;
    use crate::wordml::BlockKind;
    let mut tx = state.begin();
    let keys: Vec<(crate::wordml::BlockKey, usize)> = {
        let view = tx.parts();
        let main = state.parts().story_specs().ok()?.0.first()?.part.clone();
        let id = view.id(&main)?;
        let story = view.story(id).ok()?;
        let tree = view.tree(id).ok()?;
        story
            .blocks()
            .iter()
            .enumerate()
            .filter(|(_, e)| e.kind == BlockKind::Paragraph)
            .filter_map(|(i, e)| {
                let p = crate::wordml::Paragraph::new(tree, e.node)?;
                let _ = i;
                Some((
                    story.key(e),
                    p.text(crate::api::view::Mode::Final).chars().count(),
                ))
            })
            .collect()
    };
    let mut c = Compiler::new(&mut tx, Track::Untracked).ok()?;
    for (n, (key, len)) in keys.iter().enumerate() {
        if *len < 30 {
            continue;
        }
        if n % every == 0 {
            let _ = c.replace_text(key, 5..12, "reworded text");
        } else if n % (every * 3 + 1) == 1 {
            let _ = c.delete_block(key);
        }
    }
    c.finish().ok()?;
    tx.commit().ok().map(|(s, _)| s)
}

#[test]
fn the_paragraphs_an_untracked_edit_left_alone_pair_with_themselves() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let (mut checked, mut pairs, mut wrong, mut edits_seen) = (0usize, 0usize, 0usize, 0usize);
    for entry in corpus.expected_ok().take(150) {
        let Ok(a) = State::open(entry.bytes(), &Limits::default()) else {
            continue;
        };
        let Some(b) = edited(&a, 5) else { continue };
        let Ok(pairings) = pair_documents(&a, &b, &mut Atoms::default()) else {
            continue;
        };
        let Some(body) = pairings.stories.first() else {
            continue;
        };
        for root in &body.roots {
            for c in &root.pairing.containers {
                let (Some(ca), Some(cb)) = (
                    body.model_a.containers.get(c.ca),
                    body.model_b.containers.get(c.cb),
                ) else {
                    continue;
                };
                edits_seen += ca.blocks.len().abs_diff(cb.blocks.len());
                for x in &c.pairs {
                    pairs += 1;
                    if ca.blocks[x.a].id != cb.blocks[x.b].id && !ca.blocks[x.a].block.is_blank() {
                        wrong += 1;
                    }
                }
            }
        }
        checked += 1;
    }
    println!(
        "compare recovery: {checked} documents, {pairs} pairs, {wrong} wrong, {edits_seen} blocks fewer"
    );
    assert!(checked > 30);
    assert!(wrong * 1000 <= pairs, "{wrong} wrong pairs of {pairs}");
}

#[test]
fn an_untracked_edit_is_planned_mostly_as_edits_and_deletions() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let (mut checked, mut edits, mut deletes, mut inserts, mut other) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for entry in corpus.expected_ok().take(150) {
        let Ok(a) = State::open(entry.bytes(), &Limits::default()) else {
            continue;
        };
        let Some(b) = edited(&a, 5) else { continue };
        let Ok(pairings) = pair_documents(&a, &b, &mut Atoms::default()) else {
            continue;
        };
        let Ok(plan) = super::plan::plan(&pairings, &a, &b, &|x, y| x == y) else {
            continue;
        };
        super::tests_plan::assert_covered(&pairings, &plan);
        for sp in &plan.stories {
            for e in &sp.entries {
                match e {
                    super::plan::Entry::Edit(_) => edits += 1,
                    super::plan::Entry::Delete { .. } => deletes += 1,
                    super::plan::Entry::Insert { .. } => inserts += 1,
                    _ => other += 1,
                }
            }
        }
        checked += 1;
    }
    println!(
        "compare plan: {checked} documents, {edits} edits, {deletes} deletes, {inserts} inserts, {other} other"
    );
    assert!(checked > 30 && edits > 0);
    // a short paragraph that is reworded can fall under the similarity floor and become a deletion and an insertion
    assert!(
        inserts * 20 <= edits,
        "{inserts} insertions for {edits} edits"
    );
    assert!(
        other * 20 <= edits,
        "{other} splits and merges for {edits} edits"
    );
}
