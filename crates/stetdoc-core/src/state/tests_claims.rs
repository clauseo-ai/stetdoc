//! Tests of what a batch claims: stamps on the blocks that changed, forks and lineages, the edit
//! path of resolution, and readable files' tokens.

use super::tests::*;
use super::tests_edit::refs;
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::{BlockRef, Claim, Reason};

fn r(token: &str) -> BlockRef {
    BlockRef::parse(token).unwrap()
}

fn open(texts: &[&str]) -> State {
    state(&paragraphs(texts))
}

#[test]
fn touched_blocks_new_blocks_and_moved_blocks_are_stamped() {
    let s = open(&["a", "b", "c", "d"]);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    tx.touch(&keys[2]);
    tx.touch(&keys[2]);
    assert_eq!(tx.touched(), [keys[2].clone()], "no repeats");
    // a moves to the end: detach and insert the same element again. The blocks that kept their
    // order are b, c and d, so a is the one that moved.
    let tree = tx.part_mut(MAIN).unwrap();
    let parent = tree.node_ref(keys[0].node()).parent().unwrap().id();
    let end = tree.node_ref(keys[3].node()).next_sibling().map(|n| n.id());
    tree.edit().detach(keys[0].node()).unwrap();
    tree.edit()
        .insert_before(parent, end, keys[0].node())
        .unwrap();
    let n = new_paragraph(&mut tx, "e");
    put_after(&mut tx, n, &keys[1]);
    let (next, _) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["p2", "p5", "p3", "p4", "p1"]);
    let v1 = next.base_token();
    let stale = |token: &str, claim: &BaseToken| next.begin().resolve(Claim::Base(claim), token);
    // Touched, moved and new: refused for a batch written at version 0, accepted at version 1.
    for token in ["p3", "p1", "p5"] {
        let e = stale(token, &v0).unwrap_err();
        assert_eq!(e.code(), ErrorCode::StaleRef, "{token}");
        assert!(
            e.message()
                .contains("changed in version 1, after version 0"),
            "{token}: {}",
            e.message()
        );
        assert!(stale(token, &v1).is_ok(), "{token}");
    }
    // Untouched blocks are fine at version 0.
    for token in ["p2", "p4"] {
        assert!(stale(token, &v0).is_ok(), "{token}");
    }
}

#[test]
fn the_paragraph_after_a_changed_mark_revision_is_stamped() {
    let s = open(&["a", "b", "c", "d"]);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    // b gets a deleted paragraph mark: it joins c in the final view, so c's text changes though
    // c's bytes do not.
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let mut e = tree.edit();
        let ppr = e.create_element(W, "pPr").unwrap();
        let rpr = e.create_element(W, "rPr").unwrap();
        let del = e.create_element(W, "del").unwrap();
        e.set_attr(del, W, "id", "1").unwrap();
        e.set_attr(del, W, "author", "a").unwrap();
        e.set_attr(del, W, "date", "2024-01-01T00:00:00Z").unwrap();
        e.append_child(rpr, del).unwrap();
        e.append_child(ppr, rpr).unwrap();
        let first = tree.node_ref(keys[1].node()).first_child().map(|n| n.id());
        tree.edit()
            .insert_before(keys[1].node(), first, ppr)
            .unwrap();
    }
    tx.touch(&keys[1]);
    let (next, _) = tx.commit().unwrap();
    let at =
        |token: &str, claim: &BaseToken| next.begin().resolve(Claim::Base(claim), token).is_ok();
    assert!(!at("p2", &v0), "b changed");
    assert!(!at("p3", &v0), "c follows a changed mark");
    assert!(at("p1", &v0));
    assert!(at("p4", &v0), "only the next paragraph is stamped");
}

#[test]
fn the_audit_stamps_the_paragraphs_whose_field_start_changed() {
    // p1 opens a field that p2 closes. Deleting p1 changes how p2 starts, and p2's bytes do not
    // change.
    let begin = "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r>";
    let end = "<w:r><w:t>tail</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
    let body = format!("<w:p>{begin}</w:p><w:p>{end}</w:p>{}", para_xml("c"));
    let s = state(&body);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    // Nothing has read the text, so the base story has no field pass yet: the audit runs it.
    let mut tx = s.begin();
    detach(&mut tx, &keys[0]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(committed.retired, [(r("p1"), Reason::Deleted)]);
    let at = |token: &str| next.begin().resolve(Claim::Base(&v0), token).is_ok();
    assert!(!at("p2"), "its field start changed");
    assert!(at("p3"), "nothing about c changed");
}

#[test]
fn forks_of_one_document_get_different_lineages_and_equal_edits_get_equal_ones() {
    let s = open(&["a", "b"]);
    let keys = body_keys(&s);
    let insert = |text: &str| {
        let mut tx = s.begin();
        let n = new_paragraph(&mut tx, text);
        put_after(&mut tx, n, &keys[1]);
        tx.commit().unwrap().0
    };
    let (x, y, x2) = (
        insert("Governing law"),
        insert("Notices"),
        insert("Governing law"),
    );
    assert_eq!(refs(&x), refs(&y), "both mint p3 for a different paragraph");
    assert_eq!(x.base_token().version(), y.base_token().version());
    assert_ne!(x.base_token(), y.base_token());
    assert_eq!(
        x.base_token(),
        x2.base_token(),
        "byte-identical edits give the same lineage"
    );
    // A batch from the first fork is refused by the second, at the same version.
    let from_x = x.base_token();
    let e = y.begin().resolve(Claim::Base(&from_x), "p3").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert!(e.message().contains("different history"), "{}", e.message());
    assert!(x2.begin().resolve(Claim::Base(&from_x), "p3").is_ok());
}

#[test]
fn an_edit_must_name_the_base_token_and_resolves_against_its_own_declarations() {
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let claim = s.base_token();
    let mut tx = s.begin();
    // `Implied` is refused before anything else, even for a token that does not parse.
    for token in ["p1", "banana"] {
        let e = tx.resolve(Claim::Implied, token).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidPayload);
        assert!(e.next().contains("base token"), "{}", e.next());
    }
    // The checks of the read path apply.
    let other = open(&["x"]).base_token();
    assert_eq!(
        tx.resolve(Claim::Base(&other), "p1").unwrap_err().code(),
        ErrorCode::StaleRef
    );
    assert_eq!(
        tx.resolve(Claim::Base(&claim), "p9").unwrap_err().code(),
        ErrorCode::NotFound
    );
    assert_eq!(
        tx.resolve(Claim::Base(&claim), "p1x").unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    assert_eq!(tx.resolve(Claim::Base(&claim), "p2").unwrap(), keys[1]);
    // A block that an earlier operation detached is stale, whether or not it was declared.
    detach(&mut tx, &keys[1]);
    let e = tx.resolve(Claim::Base(&claim), "p2").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert_eq!(e.message(), "p2 was deleted earlier in this batch.");
    // A declared retirement says why.
    tx.retire(&keys[2], Retired::Rejected).unwrap();
    assert!(
        tx.resolve(Claim::Base(&claim), "p3")
            .unwrap_err()
            .message()
            .contains("insertion was rejected")
    );
    // The block that was not touched still resolves, and the state was never written to.
    assert_eq!(tx.resolve(Claim::Base(&claim), "p1").unwrap(), keys[0]);
    assert_eq!(s.resolve(Claim::Implied, "p2").unwrap(), keys[1]);
}

#[test]
fn a_ref_minted_after_the_claims_version_is_refused_and_so_is_a_stale_version() {
    let s = open(&["a"]);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "b");
    put_after(&mut tx, n, &keys[0]);
    let (one, _) = tx.commit().unwrap();
    let mut tx = one.begin();
    tx.touch(&body_keys(&one)[0]);
    let (two, _) = tx.commit().unwrap();
    // A claim from version 0 names p2, which was minted in version 1.
    let e = two.begin().resolve(Claim::Base(&v0), "p2").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    // A claim from version 1 is fine for p2 and refused for p1, which version 2 changed.
    let v1 = one.base_token();
    assert!(two.begin().resolve(Claim::Base(&v1), "p2").is_ok());
    assert!(two.begin().resolve(Claim::Base(&v1), "p1").is_err());
    // A claim from the future is refused.
    let v2 = two.base_token();
    let e = one.begin().resolve(Claim::Base(&v2), "p1").unwrap_err();
    assert!(
        e.message().contains("version 2") && e.message().contains("version 1"),
        "{}",
        e.message()
    );
}

#[test]
fn readable_files_must_carry_the_current_token() {
    let s = open(&["a"]);
    let v0 = s.base_token();
    assert!(s.check_files_base(&v0).is_ok());
    let mut tx = s.begin();
    tx.touch(&body_keys(&s)[0]);
    let (one, _) = tx.commit().unwrap();
    let e = one.check_files_base(&v0).unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleFiles);
    assert!(
        e.message()
            .contains("exported at version 0, and the document is at version 1"),
        "{}",
        e.message()
    );
    let v1 = one.base_token();
    assert!(one.check_files_base(&v1).is_ok());
    let e = s.check_files_base(&v1).unwrap_err();
    assert!(e.message().contains("another copy"), "{}", e.message());
    let other = open(&["x"]).base_token();
    assert!(
        one.check_files_base(&other)
            .unwrap_err()
            .message()
            .contains("another document")
    );
    // The same version with another history.
    let mut fork = s.begin();
    let n = new_paragraph(&mut fork, "new");
    put_after(&mut fork, n, &body_keys(&s)[0]);
    let (forked, _) = fork.commit().unwrap();
    assert_eq!(forked.base_token().version(), v1.version());
    assert!(
        one.check_files_base(&forked.base_token())
            .unwrap_err()
            .message()
            .contains("different history")
    );
}

#[test]
fn a_resumed_state_stamps_field_starts_too() {
    // The same, after export and resume, which leaves every story without a field pass.
    let begin = "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r>";
    let end = "<w:r><w:t>tail</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
    let body = format!("<w:p>{begin}</w:p><w:p>{end}</w:p>{}", para_xml("c"));
    let original = package_bytes(&body);
    let s = state_from(original.clone(), &Limits::default());
    let e = s.export().unwrap();
    let rec = Recorded {
        base: e.base,
        head_sha256: e.head_sha256,
        refs_sha256: e.refs_sha256,
    };
    let back = State::resume(e.head, &e.refs, &rec, Some(&original), &Limits::default()).unwrap();
    let v0 = back.base_token();
    let keys = body_keys(&back);
    let mut tx = back.begin();
    detach(&mut tx, &keys[0]);
    let (next, _) = tx.commit().unwrap();
    let at = |token: &str| next.begin().resolve(Claim::Base(&v0), token).is_ok();
    assert!(!at("p2"), "its field start changed");
    assert!(at("p3"));
}

#[test]
fn a_view_says_which_ref_a_block_has_as_the_declarations_leave_it() {
    let s = open(&["a", "b", "c"]);
    let keys = body_keys(&s);
    let view = s.parts();
    assert_eq!(view.ref_of(&keys[0]), Some(r("p1")));
    assert_eq!(view.ref_of(&keys[2]), Some(r("p3")));
    // A transaction's view: a new block has no ref, because refs are minted at commit.
    let mut tx = s.begin();
    let n = new_paragraph(&mut tx, "new");
    let fresh = put_after(&mut tx, n, &keys[2]);
    assert_eq!(tx.parts().ref_of(&fresh), None);
    assert_eq!(tx.parts().ref_of(&keys[1]), Some(r("p2")));
    // A merge that keeps the absorbed block's element moves the survivor's ref onto it.
    tx.merge(&keys[0], &keys[1], keys[1].clone()).unwrap();
    assert_eq!(
        tx.parts().ref_of(&keys[1]),
        Some(r("p1")),
        "the survivor's ref names it now"
    );
    assert_eq!(
        tx.parts().ref_of(&keys[0]),
        None,
        "and the old element has none"
    );
    // The committed state's own view is untouched by the transaction, and agrees after the commit.
    assert_eq!(s.parts().ref_of(&keys[0]), Some(r("p1")));
    detach(&mut tx, &keys[0]);
    let (next, _) = tx.commit().unwrap();
    assert_eq!(next.parts().ref_of(&keys[1]), Some(r("p1")));
    assert_eq!(next.parts().ref_of(&fresh), Some(r("p4")));
    assert_eq!(next.parts().ref_of(&keys[0]), None);
}
