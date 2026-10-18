//! Tests of merges: two blocks and three, in every order and with every choice of element, the
//! composition the audit checks, and the declarations it refuses.

use super::tests::*;
use super::tests_edit::{refs, snapshot};
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::{BlockRef, Claim};

fn r(token: &str) -> BlockRef {
    BlockRef::parse(token).unwrap()
}

fn open(texts: &[&str]) -> State {
    state(&paragraphs(texts))
}

fn resolve_error(s: &State, token: &str) -> StetError {
    s.resolve(Claim::Implied, token).unwrap_err()
}

/// Which element a merge keeps.
#[derive(Clone, Copy, Debug)]
enum Keep {
    First,
    Second,
    New,
}

#[test]
fn a_merge_keeps_either_element_or_a_new_one_and_the_first_ref() {
    for keep in [Keep::First, Keep::Second, Keep::New] {
        let s = open(&["a", "b", "c"]);
        let keys = body_keys(&s);
        let mut tx = s.begin();
        let into = match keep {
            Keep::First => {
                detach(&mut tx, &keys[1]);
                keys[0].clone()
            }
            Keep::Second => {
                detach(&mut tx, &keys[0]);
                keys[1].clone()
            }
            Keep::New => {
                let n = new_paragraph(&mut tx, "merged");
                let k = put_before(&mut tx, n, &keys[0]);
                detach(&mut tx, &keys[0]);
                detach(&mut tx, &keys[1]);
                k
            }
        };
        tx.merge(&keys[0], &keys[1], into.clone()).unwrap();
        let (next, committed) = tx.commit().unwrap();
        assert_eq!(
            next.resolve(Claim::Implied, "p1").unwrap(),
            into,
            "{keep:?}"
        );
        let e = resolve_error(&next, "p2");
        assert_eq!(
            e.message(),
            "p2 was merged into p1 in version 1.",
            "{keep:?}"
        );
        assert_eq!(
            next.resolve(Claim::Implied, "p3").unwrap(),
            keys[2],
            "{keep:?}"
        );
        assert!(committed.minted.is_empty(), "{keep:?}: nothing is new");
        assert_eq!(committed.merged, [(r("p1"), r("p2"))]);
        assert_eq!(refs(&next), ["p1", "p3"], "{keep:?}");
        // The merged ref survives export and resume, with its tombstone.
        let saved = next.export().unwrap();
        let recorded = Recorded {
            base: saved.base,
            head_sha256: saved.head_sha256,
            refs_sha256: saved.refs_sha256,
        };
        let back = State::resume(
            saved.head,
            &saved.refs,
            &recorded,
            Some(&package_bytes(&paragraphs(&["a", "b", "c"]))),
            &Limits::default(),
        )
        .unwrap();
        assert_eq!(
            back.resolve(Claim::Implied, "p2").unwrap_err().message(),
            e.message()
        );
    }
}

/// The two orders in which three blocks can be merged with two declarations.
#[derive(Clone, Copy, Debug)]
enum Order {
    /// `merge(a, b)` and then `merge(a, c)`: accepting two consecutive deleted marks.
    LeftFirst,
    /// `merge(b, c)` and then `merge(a, b)`.
    RightFirst,
}

/// Merges `p1`, `p2` and `p3` of four paragraphs in the given order, where each declaration's `into`
/// is one of the three choices (the survivor's element, the absorbed element, or a new one), and
/// checks the composition: the survivor ends on the last `into`, both others are merged into it,
/// and nothing is minted or lost.
fn merge_three(order: Order, first: usize, second: usize) {
    let s = open(&["a", "b", "c", "d"]);
    let keys = body_keys(&s);
    let (a, b, c) = (keys[0].clone(), keys[1].clone(), keys[2].clone());
    let mut tx = s.begin();
    let mut created = Vec::new();
    let mut fresh = |tx: &mut Tx<'_>| {
        let n = new_paragraph(tx, "new");
        let k = put_before(tx, n, &a);
        created.push(k.clone());
        k
    };
    // `first` and `second` pick the survivor's element (0), the absorbed one (1) or a new one (2).
    let (s1, a1, s2, a2);
    let (into1, into2);
    match order {
        Order::LeftFirst => {
            (s1, a1) = (a.clone(), b.clone());
            into1 = match first {
                0 => a.clone(),
                1 => b.clone(),
                _ => fresh(&mut tx),
            };
            tx.merge(&s1, &a1, into1.clone()).unwrap();
            (s2, a2) = (into1.clone(), c.clone());
            into2 = match second {
                0 => into1.clone(),
                1 => c.clone(),
                _ => fresh(&mut tx),
            };
            tx.merge(&s2, &a2, into2.clone()).unwrap();
        }
        Order::RightFirst => {
            (s1, a1) = (b.clone(), c.clone());
            into1 = match first {
                0 => b.clone(),
                1 => c.clone(),
                _ => fresh(&mut tx),
            };
            tx.merge(&s1, &a1, into1.clone()).unwrap();
            (s2, a2) = (a.clone(), into1.clone());
            into2 = match second {
                0 => a.clone(),
                1 => into1.clone(),
                _ => fresh(&mut tx),
            };
            tx.merge(&s2, &a2, into2.clone()).unwrap();
        }
    }
    // Everything that is not the result leaves the tree.
    for key in [&a, &b, &c].into_iter().chain(created.iter()) {
        if *key != into2 {
            detach(&mut tx, key);
        }
    }
    let label = format!("{order:?} {first} {second}");
    let (next, committed) = tx.commit().unwrap_or_else(|e| panic!("{label}: {e}"));
    let survivor = match order {
        Order::LeftFirst => "p1",
        Order::RightFirst => "p1",
    };
    assert_eq!(
        next.resolve(Claim::Implied, survivor).unwrap(),
        into2,
        "{label}"
    );
    for gone in ["p2", "p3"] {
        let e = resolve_error(&next, gone);
        assert_eq!(e.code(), ErrorCode::StaleRef, "{label}");
        assert_eq!(
            e.message(),
            format!("{gone} was merged into p1 in version 1."),
            "{label}"
        );
    }
    assert_eq!(
        next.resolve(Claim::Implied, "p4").unwrap(),
        keys[3],
        "{label}"
    );
    assert!(
        committed.minted.is_empty(),
        "{label}: nothing is new: {:?}",
        committed.minted
    );
    assert!(committed.retired.is_empty(), "{label}");
    assert_eq!(committed.merged.len(), 2, "{label}");
    assert_eq!(refs(&next), ["p1", "p4"], "{label}");
    // The refs are one to one with the blocks, and the table survives a round trip: the tombstones
    // of one version never form a chain.
    let saved = next.export().unwrap();
    let recorded = Recorded {
        base: saved.base,
        head_sha256: saved.head_sha256,
        refs_sha256: saved.refs_sha256,
    };
    let original = package_bytes(&paragraphs(&["a", "b", "c", "d"]));
    let back = State::resume(
        saved.head,
        &saved.refs,
        &recorded,
        Some(&original),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("{label}: resume: {e}"));
    assert_eq!(back.base_token(), next.base_token(), "{label}");
    for gone in ["p2", "p3"] {
        assert_eq!(
            resolve_error(&back, gone).message(),
            format!("{gone} was merged into p1 in version 1."),
            "{label}"
        );
    }
}

#[test]
fn a_merge_of_three_blocks_is_two_declarations_in_either_order_and_every_choice_of_element() {
    for order in [Order::LeftFirst, Order::RightFirst] {
        for first in 0..3 {
            for second in 0..3 {
                merge_three(order, first, second);
            }
        }
    }
}

#[test]
fn accepting_two_consecutive_deleted_marks_keeps_the_last_element() {
    // The case from the review: merge(p1, p2, into: e2), then merge(p1, p3, into: e3).
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    tx.merge(&keys[0], &keys[1], keys[1].clone()).unwrap();
    // After the first declaration the ref p1 names the second element.
    let claim = s.base_token();
    assert_eq!(tx.resolve(Claim::Base(&claim), "p1").unwrap(), keys[1]);
    let e = tx.resolve(Claim::Base(&claim), "p2").unwrap_err();
    assert_eq!(e.message(), "p2 was merged into p1 earlier in this batch.");
    tx.merge(&keys[1], &keys[2], keys[2].clone()).unwrap();
    assert!(matches!(
        tx.intents(),
        [Intent::Merge { .. }, Intent::Merge { .. }]
    ));
    detach(&mut tx, &keys[0]);
    detach(&mut tx, &keys[1]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(next.resolve(Claim::Implied, "p1").unwrap(), keys[2]);
    assert_eq!(
        resolve_error(&next, "p3").message(),
        "p3 was merged into p1 in version 1."
    );
    assert_eq!(committed.merged, [(r("p1"), r("p2")), (r("p1"), r("p3"))]);
}

#[test]
fn accepting_two_consecutive_deleted_marks_joins_into_the_first_element() {
    // What `revisions` declares: p6 absorbs p7 on its own element, and then p5 absorbs p6 and keeps
    // its own element, so merge(p6, p7, into: e6) and then merge(p5, p6, into: e5).
    let s = open(&["1", "2", "3", "4", "5", "6", "7"]);
    let keys = body_keys(&s);
    let (e5, e6, e7) = (keys[4].clone(), keys[5].clone(), keys[6].clone());
    let mut tx = s.begin();
    tx.merge(&e6, &e7, e6.clone()).unwrap();
    // p6 names its own element, and p7 is merged into it.
    assert_eq!(tx.resolve(Claim::Base(&s.base_token()), "p6").unwrap(), e6);
    let e = tx.resolve(Claim::Base(&s.base_token()), "p7").unwrap_err();
    assert_eq!(e.message(), "p7 was merged into p6 earlier in this batch.");
    tx.merge(&e5, &e6, e5.clone()).unwrap();
    let e = tx.resolve(Claim::Base(&s.base_token()), "p7").unwrap_err();
    assert_eq!(
        e.message(),
        "p7 was merged into p5 earlier in this batch.",
        "the first merge's tombstone follows the second"
    );
    detach(&mut tx, &e6);
    detach(&mut tx, &e7);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(next.resolve(Claim::Implied, "p5").unwrap(), e5);
    for gone in ["p6", "p7"] {
        assert_eq!(
            resolve_error(&next, gone).message(),
            format!("{gone} was merged into p5 in version 1.")
        );
    }
    assert_eq!(refs(&next), ["p1", "p2", "p3", "p4", "p5"]);
    assert_eq!(committed.merged, [(r("p6"), r("p7")), (r("p5"), r("p6"))]);
    assert!(committed.minted.is_empty());
}

#[test]
fn an_element_that_one_declaration_released_and_a_later_one_took_back_may_stay() {
    // merge(p1, p2) into a new element, and then merge that block with p3 into p2's old element:
    // the first declaration released it, and the second took it back, so it stays in the tree.
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "new");
    let fresh = put_before(&mut tx, n, &keys[0]);
    tx.merge(&keys[0], &keys[1], fresh.clone()).unwrap();
    tx.merge(&fresh, &keys[2], keys[1].clone()).unwrap();
    detach(&mut tx, &keys[0]);
    detach(&mut tx, &keys[2]);
    detach(&mut tx, &fresh);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(next.resolve(Claim::Implied, "p1").unwrap(), keys[1]);
    assert_eq!(
        resolve_error(&next, "p2").message(),
        "p2 was merged into p1 in version 1."
    );
    assert_eq!(
        resolve_error(&next, "p3").message(),
        "p3 was merged into p1 in version 1."
    );
    assert!(committed.minted.is_empty());
    assert_eq!(refs(&next), ["p1"]);
    // And if the element that was released is not taken back, it must be gone.
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "new");
    let fresh = put_before(&mut tx, n, &keys[0]);
    tx.merge(&keys[0], &keys[1], fresh.clone()).unwrap();
    detach(&mut tx, &keys[0]);
    // p2's element was released by the merge, and it is still in the tree.
    let e = tx.commit().err().unwrap();
    assert_eq!(e.code(), ErrorCode::Internal);
}

#[test]
fn a_rebind_whose_target_a_later_merge_absorbs_is_valid() {
    // A split keeps the original element for the second part, and then the first part is merged
    // into the paragraph before it.
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "first part");
    let first = put_before(&mut tx, n, &keys[1]);
    tx.rebind(&keys[1], first.clone()).unwrap();
    // p2 now names `first`. Merge it into p1, keeping p1's element.
    tx.merge(&keys[0], &first, keys[0].clone()).unwrap();
    detach(&mut tx, &first);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(next.resolve(Claim::Implied, "p1").unwrap(), keys[0]);
    assert_eq!(
        resolve_error(&next, "p2").message(),
        "p2 was merged into p1 in version 1."
    );
    // The original element of the split is a new block, minted p4.
    assert_eq!(committed.minted, [(keys[1].clone(), r("p4"))]);
    assert_eq!(refs(&next), ["p1", "p4", "p3"]);
}

/// A transaction fails to commit with `internal`, and the state it began on is as it was.
fn contradiction(
    texts: &[&str],
    build: impl FnOnce(&mut Tx<'_>, &[BlockKey]) -> Result<(), StetError>,
) {
    let s = open(texts);
    let before = snapshot(&s);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let outcome = build(&mut tx, &keys).and_then(|()| tx.commit().map(|_| ()));
    let e = outcome.unwrap_err();
    assert_eq!(e.code(), ErrorCode::Internal, "{e}");
    assert!(e.message().starts_with("internal:"), "{e}");
    assert_eq!(snapshot(&s), before, "the state is unchanged");
    assert_eq!(refs(&s).len(), keys.len());
}

#[test]
fn a_declaration_that_contradicts_the_trees_is_internal_and_changes_nothing() {
    // A retired block whose element is still in the tree.
    contradiction(&["a", "b"], |tx, k| tx.retire(&k[1], Retired::Rejected));
    // A merge whose released element is still attached.
    contradiction(&["a", "b"], |tx, k| tx.merge(&k[0], &k[1], k[0].clone()));
    contradiction(&["a", "b"], |tx, k| {
        detach(tx, &k[0]);
        tx.merge(&k[0], &k[1], k[0].clone())
    });
    // A rebind to an element that is not a block.
    contradiction(&["a", "b"], |tx, k| {
        let run = {
            let tree = tx.part_mut(MAIN).unwrap();
            tree.node_ref(k[0].node()).first_child().unwrap().id()
        };
        let to = key_in(tx, run);
        tx.rebind(&k[0], to)
    });
    // A rebind of a paragraph ref to a table.
    contradiction(&["a", "b"], |tx, k| {
        let table = {
            let tree = tx.part_mut(MAIN).unwrap();
            let parent = tree.node_ref(k[0].node()).parent().unwrap().id();
            let t = tree.edit().create_element(W, "tbl").unwrap();
            tree.edit()
                .insert_before(parent, Some(k[0].node()), t)
                .unwrap();
            t
        };
        let to = key_in(tx, table);
        tx.rebind(&k[0], to)
    });
}

#[test]
fn a_declaration_about_a_block_with_no_live_ref_is_refused_at_once() {
    let s = open(&["a", "b"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "new");
    let fresh = put_after(&mut tx, n, &keys[1]);
    for e in [
        tx.retire(&fresh, Retired::Deleted).unwrap_err(),
        tx.rebind(&fresh, keys[0].clone()).unwrap_err(),
        tx.merge(&fresh, &keys[0], fresh.clone()).unwrap_err(),
        tx.merge(&keys[0], &keys[0], keys[0].clone()).unwrap_err(),
        tx.merge(&keys[0], &keys[1], keys[1].clone())
            .and_then(|()| tx.merge(&keys[0], &keys[1], keys[0].clone()))
            .unwrap_err(),
        tx.rebind(&keys[0], keys[1].clone()).unwrap_err(),
    ] {
        assert_eq!(e.code(), ErrorCode::Internal, "{e}");
    }
}

#[test]
fn a_merged_block_that_a_later_operation_deletes_takes_what_it_absorbed_with_it() {
    // Two merges in one batch, and then the merged block is deleted. The refs that were merged into
    // it are gone for the same reason, so no tombstone of one version points at another.
    let original = package_bytes(&paragraphs(&["a", "b", "c", "d"]));
    let s = state_from(original.clone(), &Limits::default());
    let keys = body_keys(&s);
    let mut tx = s.begin();
    detach(&mut tx, &keys[1]);
    tx.merge(&keys[0], &keys[1], keys[0].clone()).unwrap();
    detach(&mut tx, &keys[2]);
    tx.merge(&keys[0], &keys[2], keys[0].clone()).unwrap();
    detach(&mut tx, &keys[0]);
    let (next, committed) = tx.commit().unwrap();
    for gone in ["p1", "p2", "p3"] {
        assert_eq!(
            resolve_error(&next, gone).message(),
            format!("{gone} was deleted in version 1.")
        );
    }
    assert_eq!(refs(&next), ["p4"]);
    assert_eq!(committed.retired, [(r("p1"), crate::refs::Reason::Deleted)]);
    // The table can be saved and restored: nothing in it breaks the rule that a merge target is
    // live or was retired in a later version.
    let saved = next.export().unwrap();
    let rec = Recorded {
        base: saved.base,
        head_sha256: saved.head_sha256,
        refs_sha256: saved.refs_sha256,
    };
    let back = State::resume(
        saved.head,
        &saved.refs,
        &rec,
        Some(&original),
        &Limits::default(),
    )
    .unwrap();
    assert_eq!(
        resolve_error(&back, "p2").message(),
        "p2 was deleted in version 1."
    );
}

#[test]
fn a_ref_bound_to_an_element_that_is_not_in_the_document_is_retired_as_deleted() {
    // A merge into an element that never joined the tree: the block it names is not in the
    // document, so its ref is gone, like the ref of any block that is not there.
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let floating = {
        let n = new_paragraph(&mut tx, "floating");
        key_in(&tx, n)
    };
    detach(&mut tx, &keys[0]);
    detach(&mut tx, &keys[1]);
    tx.merge(&keys[0], &keys[1], floating).unwrap();
    let (next, _) = tx.commit().unwrap();
    assert_eq!(
        resolve_error(&next, "p1").message(),
        "p1 was deleted in version 1."
    );
    assert_eq!(
        resolve_error(&next, "p2").message(),
        "p2 was deleted in version 1."
    );
    assert_eq!(refs(&next), ["p3"]);
    // The same for a rebind: the old element is still in the tree, so it is a new block.
    let mut tx = s.begin();
    let floating = {
        let n = new_paragraph(&mut tx, "floating");
        key_in(&tx, n)
    };
    tx.rebind(&keys[0], floating).unwrap();
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        resolve_error(&next, "p1").message(),
        "p1 was deleted in version 1."
    );
    assert_eq!(committed.minted, [(keys[0].clone(), r("p4"))]);
    assert_eq!(refs(&next), ["p4", "p2", "p3"]);
}
