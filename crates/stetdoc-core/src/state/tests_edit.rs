//! Tests of commit and the audit: what happens to refs when elements are inserted, deleted and
//! split, and what a commit that changes nothing does.

use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::{BlockRef, Claim, Reason};

pub(super) fn open(texts: &[&str]) -> State {
    state(&paragraphs(texts))
}

/// The ref of each block of the body, in order, or `-` for a block with none.
pub(super) fn refs(s: &State) -> Vec<String> {
    body_keys(s)
        .iter()
        .map(|k| s.refs.ref_of(k).map_or("-".to_owned(), |r| r.to_string()))
        .collect()
}

/// Everything that a state's saved form shows, to compare a state with itself later.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    head: Vec<u8>,
    refs: Vec<u8>,
    token: String,
    generations: Vec<(u64, u64)>,
}

pub(super) fn snapshot(s: &State) -> Snapshot {
    let e = s.export().unwrap();
    Snapshot {
        head: e.head,
        refs: e.refs,
        token: e.base.to_string(),
        generations: s
            .slots
            .iter()
            .map(|slot| (slot.generation.epoch, slot.generation.edits))
            .collect(),
    }
}

fn r(token: &str) -> BlockRef {
    BlockRef::parse(token).unwrap()
}

fn resolve_error(s: &State, token: &str) -> crate::api::error::StetError {
    s.resolve(Claim::Implied, token).unwrap_err()
}

#[test]
fn open_mints_refs_in_document_order_and_the_same_bytes_give_the_same_refs() {
    let a = open(&["a", "b", "c"]);
    assert_eq!(refs(&a), ["p1", "p2", "p3"]);
    assert_eq!(a.base_token().version(), 0);
    let b = open(&["a", "b", "c"]);
    assert_eq!(a.base_token(), b.base_token());
    assert_eq!(refs(&a), refs(&b));
    let c = open(&["a", "b", "d"]);
    assert_ne!(a.base_token().namespace(), c.base_token().namespace());
    assert_eq!(a.resolve(Claim::Implied, "p2").unwrap(), body_keys(&a)[1]);
}

#[test]
fn a_commit_mints_in_document_order_after_every_number_issued() {
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    // The later block is inserted first, so the order of the operations is not the document's.
    let late = new_paragraph(&mut tx, "late");
    let late_key = put_after(&mut tx, late, &keys[2]);
    let early = new_paragraph(&mut tx, "early");
    let early_key = put_before(&mut tx, early, &keys[0]);
    let (next, committed) = tx.commit().unwrap();
    let minted: Vec<(BlockKey, String)> = committed
        .minted
        .iter()
        .map(|(k, r)| (k.clone(), r.to_string()))
        .collect();
    assert_eq!(
        minted,
        [(early_key, "p4".to_owned()), (late_key, "p5".to_owned())]
    );
    assert_eq!(refs(&next), ["p4", "p1", "p2", "p3", "p5"]);
    assert_eq!(next.base_token().version(), 1);
    assert_eq!(committed.base, next.base_token());
    assert!(committed.retired.is_empty() && committed.merged.is_empty());
    // The base is as it was.
    assert_eq!(refs(&s), ["p1", "p2", "p3"]);
    assert_eq!(s.base_token().version(), 0);
}

#[test]
fn a_dry_run_and_a_discard_touch_nothing_and_a_later_edit_mints_the_same_refs() {
    let s = open(&["a", "b"]);
    let before = snapshot(&s);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "dry");
    put_after(&mut tx, n, &keys[1]);
    detach(&mut tx, &keys[0]);
    tx.touch(&keys[1]);
    // The dry run's receipt would have had a new block with no ref, and nothing was minted.
    assert_eq!(refs(&s), ["p1", "p2"]);
    tx.discard();
    assert_eq!(snapshot(&s), before);
    // The real edit mints p3 for the same insertion: the discarded one never took it.
    let mut real = s.begin();
    let n = new_paragraph(&mut real, "real");
    let key = put_after(&mut real, n, &keys[1]);
    let (next, committed) = real.commit().unwrap();
    assert_eq!(committed.minted, [(key, r("p3"))]);
    assert_eq!(refs(&next), ["p1", "p2", "p3"]);
}

#[test]
fn deleting_a_paragraph_retires_its_ref_for_good() {
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    detach(&mut tx, &keys[1]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(committed.retired, [(r("p2"), Reason::Deleted)]);
    assert_eq!(refs(&next), ["p1", "p3"]);
    let e = resolve_error(&next, "p2");
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert_eq!(e.message(), "p2 was deleted in version 1.");
    // The number is not reused, whatever is inserted where the block was.
    let mut tx = next.begin();
    let n = new_paragraph(&mut tx, "again");
    put_before(&mut tx, n, &body_keys(&next)[1]);
    let (after, committed) = tx.commit().unwrap();
    assert_eq!(
        committed
            .minted
            .iter()
            .map(|(_, r)| r.to_string())
            .collect::<Vec<_>>(),
        ["p4"]
    );
    assert_eq!(refs(&after), ["p1", "p4", "p3"]);
    assert_eq!(resolve_error(&after, "p2").code(), ErrorCode::StaleRef);
}

#[test]
fn a_rejected_insertion_is_recorded_as_rejected() {
    let s = open(&["a", "b"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    tx.retire(&keys[1], Retired::Rejected).unwrap();
    detach(&mut tx, &keys[1]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(committed.retired, [(r("p2"), Reason::Rejected)]);
    assert!(
        resolve_error(&next, "p2")
            .message()
            .contains("insertion was rejected")
    );
}

#[test]
fn a_tracked_deletion_keeps_its_ref() {
    let deleted = "<w:p><w:pPr><w:rPr><w:del w:id=\"1\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"/></w:rPr></w:pPr><w:del w:id=\"2\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"><w:r><w:delText>gone</w:delText></w:r></w:del></w:p>";
    let s = state(&format!("{}{deleted}{}", para_xml("a"), para_xml("c")));
    assert_eq!(refs(&s), ["p1", "p2", "p3"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    tx.touch(&keys[0]);
    let (next, _) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["p1", "p2", "p3"]);
    assert!(
        next.resolve(Claim::Implied, "p2").is_ok(),
        "the element is still in the document"
    );
}

#[test]
fn a_split_that_keeps_the_original_element_for_the_first_part_declares_nothing() {
    let s = open(&["a", "b"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let second = new_paragraph(&mut tx, "second part");
    let second_key = put_after(&mut tx, second, &keys[0]);
    tx.touch(&keys[0]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["p1", "p3", "p2"]);
    assert_eq!(committed.minted, [(second_key, r("p3"))]);
    assert_eq!(next.resolve(Claim::Implied, "p1").unwrap(), keys[0]);
}

#[test]
fn a_split_that_keeps_the_original_element_for_the_second_part_rebinds() {
    let s = open(&["a", "b"]);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    let first = new_paragraph(&mut tx, "first part");
    let first_key = put_before(&mut tx, first, &keys[0]);
    tx.rebind(&keys[0], first_key.clone()).unwrap();
    // Inside the batch the ref already names the first part.
    let claim = s.base_token();
    assert_eq!(tx.resolve(Claim::Base(&claim), "p1").unwrap(), first_key);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["p1", "p3", "p2"]);
    assert_eq!(committed.rebound, [(r("p1"), first_key.clone())]);
    assert_eq!(
        committed.minted,
        [(keys[0].clone(), r("p3"))],
        "the old element is a new block"
    );
    assert_eq!(next.resolve(Claim::Implied, "p1").unwrap(), first_key);
    assert_eq!(next.resolve(Claim::Implied, "p3").unwrap(), keys[0]);
}

#[test]
fn a_commit_that_changed_nothing_leaves_the_version_alone() {
    let s = open(&["a", "b"]);
    let (next, committed) = s.begin().commit().unwrap();
    assert_eq!(next.base_token(), s.base_token());
    assert_eq!(committed.base, s.base_token());
    assert!(committed.minted.is_empty());
    // A part that was borrowed and not changed does not count either.
    let mut tx = s.begin();
    tx.part_mut(MAIN).unwrap();
    let (again, _) = tx.commit().unwrap();
    assert_eq!(again.base_token(), s.base_token());
    assert!(Arc::ptr_eq(
        &again.slot(id(&again, MAIN)).unwrap().cell,
        &s.slot(id(&s, MAIN)).unwrap().cell
    ));
}

#[test]
fn a_commit_that_would_pass_the_cap_on_refs_is_refused_and_changes_nothing() {
    // Four refs may exist in all. Three are minted at open, so one more block fits and a second
    // does not, and the failure leaves the state exactly as it was.
    let limits = Limits {
        max_refs: 4,
        ..Limits::default()
    };
    let s = state_from(package_bytes(&paragraphs(&["a", "b", "c"])), &limits);
    let before = snapshot(&s);
    let keys = body_keys(&s);
    let mut tx = s.begin();
    for text in ["one", "two"] {
        let n = new_paragraph(&mut tx, text);
        put_after(&mut tx, n, &keys[2]);
    }
    let e = tx.commit().err().unwrap();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert_eq!(snapshot(&s), before);
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "one");
    put_after(&mut tx, n, &keys[2]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(committed.minted.len(), 1);
    assert_eq!(refs(&next), ["p1", "p2", "p3", "p4"]);
    // And a document that has more blocks than the cap cannot be opened.
    let too_small = Limits {
        max_refs: 2,
        ..Limits::default()
    };
    let e = State::open(package_bytes(&paragraphs(&["a", "b", "c"])), &too_small)
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
}

#[test]
fn a_transaction_that_only_borrowed_a_part_edited_before_changes_nothing() {
    let s = open(&["a", "b"]);
    let mut tx = s.begin();
    edit_first_text(&mut tx, "edited");
    let (one, _) = tx.commit().unwrap();
    assert_eq!(one.base_token().version(), 1);
    let main = id(&one, MAIN);
    assert!(
        one.parts().tree(main).unwrap().is_dirty(),
        "the part was edited in an earlier commit, and is dirty for good"
    );
    // Borrowing it changes nothing: not the version, not the lineage, not the part's generation.
    let generation = one.parts().generation(super::derived::Dep::Part(main));
    let mut tx = one.begin();
    tx.part_mut(MAIN).unwrap();
    assert_eq!(
        tx.parts().generation(super::derived::Dep::Part(main)),
        generation
    );
    let (two, committed) = tx.commit().unwrap();
    assert_eq!(two.base_token(), one.base_token());
    assert_eq!(committed.base, one.base_token());
    assert!(Arc::ptr_eq(
        &two.slot(main).unwrap().cell,
        &one.slot(main).unwrap().cell
    ));
    // Nor do calls of the editor that change nothing: a refused edit, the removal of an attribute that
    // is not there, and the binding of a namespace that the root already binds.
    let mut tx = one.begin();
    let tree = tx.part_mut(MAIN).unwrap();
    let root = tree.root().id();
    assert!(tree.edit().set_text(root, "x").is_err());
    assert!(!tree.edit().remove_attr(root, "", "absent").unwrap());
    tree.edit().bind_namespace(W, "w", false).unwrap();
    let (three, _) = tx.commit().unwrap();
    assert_eq!(three.base_token(), one.base_token());
    // A change that is real moves it, as before.
    let mut tx = one.begin();
    edit_first_text(&mut tx, "again");
    assert_eq!(tx.commit().unwrap().0.base_token().version(), 2);
}
