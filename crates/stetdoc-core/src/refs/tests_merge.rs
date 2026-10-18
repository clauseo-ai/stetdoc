//! Tests of what an edit does to the table: tombstones, rebinding and merging, including the merges
//! of one commit, which retarget each other and give a removed survivor's reason to what it absorbed.

use super::base::{BaseToken, Claim};
use super::table::{Merged, Reason};
use super::tests::{doc_table, fresh_keys};
use super::token::BlockRef;
use crate::api::error::ErrorCode;

#[test]
fn tombstone_rebind_and_merge_refuse_what_contradicts_the_table() {
    let (mut t, _, keys) = doc_table();
    let r = |s: &str| BlockRef::parse(s).unwrap();
    let fresh = fresh_keys(2);
    let internal = |e: Result<(), crate::api::error::StetError>| {
        let e = e.unwrap_err();
        assert_eq!(e.code(), ErrorCode::Internal, "{e}");
        assert!(e.message().starts_with("internal:"));
    };
    // A ref that is gone cannot be retired, rebound or merged again.
    t.tombstone(r("p1"), Reason::Deleted, 1, &Merged::default())
        .unwrap();
    internal(t.tombstone(r("p1"), Reason::Deleted, 1, &Merged::default()));
    internal(t.rebind(r("p1"), &fresh[0]));
    internal(t.merge(r("p2"), r("p1"), &keys[3], 1, &mut Merged::default()));
    internal(t.merge(r("p1"), r("p2"), &keys[3], 1, &mut Merged::default()));
    // A merge into itself, or into an element that is another block's.
    internal(t.merge(r("p2"), r("p2"), &keys[3], 1, &mut Merged::default()));
    internal(t.merge(r("p2"), r("p3"), &keys[6], 1, &mut Merged::default()));
    // Rebinding to an element that has a ref.
    internal(t.rebind(r("p2"), &keys[5]));
    // A block has one ref.
    let again = t.allocate(&keys[3], crate::wordml::BlockKind::Paragraph, 1);
    assert_eq!(again.unwrap_err().code(), ErrorCode::Internal);
    // Nothing of that changed the table.
    assert_eq!(t.ref_of(&keys[3]), Some(r("p2")));
    assert_eq!(t.ref_of(&keys[5]), Some(r("p3")));
}

#[test]
fn a_ref_absorbed_after_it_absorbed_another_in_one_commit_takes_it_along() {
    let r = |s: &str| BlockRef::parse(s).unwrap();
    let (mut t, _, keys) = doc_table();
    let mut merged = Merged::default();
    // p3 absorbs p4, and then p2 absorbs p3, all in version 1: p4 is merged into p2, not p3.
    t.merge(r("p3"), r("p4"), &keys[5], 1, &mut merged).unwrap();
    t.merge(r("p2"), r("p3"), &keys[3], 1, &mut merged).unwrap();
    for gone in ["p3", "p4"] {
        let e = t.resolve(Claim::Implied, gone).unwrap_err();
        assert_eq!(
            e.message(),
            format!("{gone} was merged into p2 in version 1.")
        );
    }
    assert!(t.resolve(Claim::Implied, "p2").is_ok());
    // In a later commit the older tombstone is history and stays as it was.
    let (mut t, _, keys) = doc_table();
    t.merge(r("p3"), r("p4"), &keys[5], 1, &mut Merged::default())
        .unwrap();
    t.merge(r("p2"), r("p3"), &keys[3], 2, &mut Merged::default())
        .unwrap();
    let e = t.resolve(Claim::Implied, "p4").unwrap_err();
    assert_eq!(
        e.message(),
        "p4 was merged into p3 in version 1, and p3 has since been removed."
    );
}

#[test]
fn a_survivor_removed_in_the_same_commit_gives_its_reason_to_what_it_absorbed() {
    let r = |s: &str| BlockRef::parse(s).unwrap();
    let (mut t, _, keys) = doc_table();
    let mut merged = Merged::default();
    t.merge(r("p2"), r("p3"), &keys[3], 1, &mut merged).unwrap();
    t.tombstone(r("p2"), Reason::Rejected, 1, &merged).unwrap();
    for gone in ["p2", "p3"] {
        let e = t.resolve(Claim::Implied, gone).unwrap_err();
        assert!(
            e.message().contains("insertion was rejected"),
            "{gone}: {}",
            e.message()
        );
    }
    // A survivor removed in a later commit leaves the earlier merge as it was.
    let (mut t, _, keys) = doc_table();
    t.merge(r("p2"), r("p3"), &keys[3], 1, &mut Merged::default())
        .unwrap();
    t.tombstone(r("p2"), Reason::Deleted, 2, &Merged::default())
        .unwrap();
    let e = t.resolve(Claim::Implied, "p3").unwrap_err();
    assert_eq!(
        e.message(),
        "p3 was merged into p2 in version 1, and p2 has since been removed."
    );
}

#[test]
fn rebind_moves_a_ref_to_a_new_element_and_leaves_the_old_one_without() {
    let (mut t, _, keys) = doc_table();
    let r = |s: &str| BlockRef::parse(s).unwrap();
    let fresh = fresh_keys(1);
    t.rebind(r("p2"), &fresh[0]).unwrap();
    assert_eq!(t.ref_of(&fresh[0]), Some(r("p2")));
    assert_eq!(t.ref_of(&keys[3]), None, "the old element has no ref now");
    assert_eq!(t.resolve(Claim::Implied, "p2").unwrap(), fresh[0]);
    // The old element can be given a new ref, as the audit does for the tail of a split.
    let minted = t
        .allocate(&keys[3], crate::wordml::BlockKind::Paragraph, 1)
        .unwrap();
    assert_eq!(minted.to_string(), "p5");
}

#[test]
fn a_merge_can_keep_either_element_or_a_new_one() {
    let r = |s: &str| BlockRef::parse(s).unwrap();
    for (name, into) in [
        ("the first element", 3usize),
        ("the second element", 5),
        ("a new element", 99),
    ] {
        let (mut t, _, keys) = doc_table();
        let fresh = fresh_keys(1);
        let into = keys.get(into).unwrap_or(&fresh[0]);
        t.merge(r("p2"), r("p3"), into, 1, &mut Merged::default())
            .unwrap();
        assert_eq!(t.ref_of(into), Some(r("p2")), "{name}");
        assert_eq!(t.resolve(Claim::Implied, "p2").unwrap(), *into, "{name}");
        assert_eq!(
            t.ref_of(&keys[3]).filter(|_| into != &keys[3]),
            None,
            "{name}"
        );
        let gone = t.resolve(Claim::Implied, "p3").unwrap_err();
        assert_eq!(
            gone.message(),
            "p3 was merged into p2 in version 1.",
            "{name}"
        );
        // The survivor changed in the merge.
        let early = BaseToken::initial(t.namespace());
        assert!(t.resolve(Claim::Base(&early), "p2").is_err(), "{name}");
    }
}
