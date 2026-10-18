//! Tests of the annotation ids that revisions and comments share: `Tx::next_annotation_id` and the
//! range of what a transaction has made.

use super::tests::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::package::NewPart;
use crate::wordml::IdKind;

/// A body whose bookmark has id 7 and whose insertion has id 12.
fn marked() -> State {
    state(
        "<w:p><w:bookmarkStart w:id=\"7\" w:name=\"a\"/><w:bookmarkEnd w:id=\"7\"/></w:p><w:p><w:ins w:id=\"12\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"><w:r><w:t>t</w:t></w:r></w:ins></w:p>",
    )
}

fn with_id(id: &str) -> State {
    state(&format!(
        "<w:p><w:bookmarkStart w:id=\"{id}\" w:name=\"a\"/><w:bookmarkEnd w:id=\"{id}\"/></w:p>"
    ))
}

fn many(tx: &mut Tx<'_>, n: usize) -> Vec<u32> {
    (0..n).map(|_| tx.next_annotation_id().unwrap()).collect()
}

/// Writes a bookmark with this id into the body of the transaction.
fn write_bookmark(tx: &mut Tx<'_>, id: u32) {
    let tree = tx.part_mut(MAIN).unwrap();
    let body = tree.root().first_child().unwrap().id();
    let mut e = tree.edit();
    let mark = e.create_element(W, "bookmarkStart").unwrap();
    e.set_attr(mark, W, "id", &id.to_string()).unwrap();
    e.set_attr(mark, W, "name", &format!("b{id}")).unwrap();
    e.append_child(body, mark).unwrap();
}

#[test]
fn the_first_annotation_id_is_one_above_the_highest_in_use_and_each_call_adds_one() {
    let s = marked();
    let mut tx = s.begin();
    assert!(tx.minted_annotation_ids().is_empty(), "nothing is made yet");
    assert_eq!(many(&mut tx, 3), [13, 14, 15]);
    assert_eq!(tx.minted_annotation_ids(), 13..16);
    assert_eq!(many(&mut tx, 1), [16]);
    assert_eq!(tx.minted_annotation_ids(), 13..17);
}

#[test]
fn a_document_that_uses_no_annotation_id_starts_at_zero() {
    let s = state(BODY);
    let mut tx = s.begin();
    assert_eq!(many(&mut tx, 3), [0, 1, 2]);
    assert_eq!(tx.minted_annotation_ids(), 0..3);
}

#[test]
fn a_transaction_that_made_nothing_has_an_empty_range_and_the_base_is_untouched() {
    let s = marked();
    let before = s.base_token();
    let mut tx = s.begin();
    assert_eq!(tx.minted_annotation_ids(), 0..0);
    let _ = many(&mut tx, 5);
    tx.discard();
    assert_eq!(s.base_token(), before);
    // A new transaction starts again from the document, not from what the last one made.
    assert_eq!(many(&mut s.begin(), 2), [13, 14]);
}

#[test]
fn a_dry_run_and_the_real_edit_make_the_same_ids() {
    let s = marked();
    let run = || {
        let mut tx = s.begin();
        let ids = many(&mut tx, 4);
        (ids, tx.minted_annotation_ids())
    };
    assert_eq!(run(), run());
}

#[test]
fn a_commit_continues_from_what_the_trees_hold() {
    let s = marked();
    let mut tx = s.begin();
    let made = many(&mut tx, 3);
    for id in &made {
        write_bookmark(&mut tx, *id);
    }
    let (next, _) = tx.commit().unwrap();
    assert_eq!(
        next.parts().ids(IdKind::AnnotationId).unwrap().max(),
        Some(15)
    );
    let mut again = next.begin();
    assert_eq!(
        many(&mut again, 2),
        [16, 17],
        "above what the last commit wrote"
    );
    // Ids that a commit did not write are free again: a transaction that makes ids and is dropped
    // leaves no trace.
    assert_eq!(many(&mut s.begin(), 1), [13]);
}

#[test]
fn the_counter_sees_the_edits_staged_and_the_parts_added_before_its_first_call() {
    let s = marked();
    // A staged edit that wrote id 40 before the first call.
    let mut tx = s.begin();
    write_bookmark(&mut tx, 40);
    assert_eq!(many(&mut tx, 2), [41, 42]);
    // A part that add_part brought in, with a comment of id 50 in it.
    let mut tx = s.begin();
    tx.add_part(NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: format!("<w:comments xmlns:w=\"{W}\"><w:comment w:id=\"50\" w:author=\"a\"><w:p><w:r><w:t>note</w:t></w:r></w:p></w:comment></w:comments>")
            .into_bytes(),
        related_from: Some((MAIN, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments")),
    })
    .unwrap();
    assert_eq!(many(&mut tx, 2), [51, 52]);
    assert_eq!(tx.minted_annotation_ids(), 51..53);
}

#[test]
fn the_range_is_exactly_what_was_made_even_when_a_part_is_added_after_the_first_call() {
    let s = marked();
    let mut tx = s.begin();
    assert_eq!(many(&mut tx, 2), [13, 14]);
    // A part added now is not read again: its ids must come from the counter. The range stays the
    // ids that were made, with nothing between them that was not.
    tx.add_part(NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: format!(
            "<w:comments xmlns:w=\"{W}\"><w:comment w:id=\"90\" w:author=\"a\"/></w:comments>"
        )
        .into_bytes(),
        related_from: Some((
            MAIN,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments",
        )),
    })
    .unwrap();
    edit_first_text(&mut tx, "changed");
    assert_eq!(many(&mut tx, 2), [15, 16]);
    assert_eq!(tx.minted_annotation_ids(), 13..17);
}

#[test]
fn past_the_largest_id_word_reads_it_is_limit_exceeded_and_nothing_changes() {
    // The document already uses the largest id.
    let s = with_id("2147483647");
    let mut tx = s.begin();
    let e = tx.next_annotation_id().unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(e.message().contains("2147483647"), "{e}");
    assert!(tx.minted_annotation_ids().is_empty());
    // An id above the signed range, which fits the unsigned scan, is the same.
    let s = with_id("4294967295");
    assert_eq!(
        s.begin().next_annotation_id().unwrap_err().code(),
        ErrorCode::LimitExceeded
    );
    // One below the largest gives the largest, once.
    let s = with_id("2147483646");
    let mut tx = s.begin();
    assert_eq!(tx.next_annotation_id().unwrap(), 2_147_483_647);
    assert_eq!(tx.minted_annotation_ids(), 2_147_483_647..2_147_483_648);
    for _ in 0..2 {
        assert_eq!(
            tx.next_annotation_id().unwrap_err().code(),
            ErrorCode::LimitExceeded
        );
    }
    assert_eq!(
        tx.minted_annotation_ids(),
        2_147_483_647..2_147_483_648,
        "a refused call does not move the range"
    );
}

#[test]
fn a_value_that_is_not_a_non_negative_whole_number_is_not_an_id_in_use() {
    // Footnote separators use -1 and 0, and a hand-written value may be anything. Only a plain
    // non-negative decimal that fits counts.
    for odd in ["-1", "abc", "1.5", "", "99999999999"] {
        let s = with_id(odd);
        assert_eq!(many(&mut s.begin(), 1), [0], "{odd:?}");
    }
}

#[test]
fn copies_are_noted_in_order_and_whoever_holds_the_transaction_sees_them() {
    let s = marked();
    let mut tx = s.begin();
    assert!(tx.copied_annotation_ids().is_empty());
    let made = many(&mut tx, 4);
    tx.note_copied_annotation_id(made[3]).unwrap();
    tx.note_copied_annotation_id(made[1]).unwrap();
    assert_eq!(
        tx.copied_annotation_ids(),
        [made[3], made[1]],
        "in the order noted"
    );
    // The minted range does not change: a copy is an id that the transaction made.
    assert_eq!(tx.minted_annotation_ids(), 13..17);
    // Another compiler over the same transaction reads the same list, and goes on adding to it.
    let seen = tx.copied_annotation_ids().to_vec();
    let more = many(&mut tx, 1);
    tx.note_copied_annotation_id(more[0]).unwrap();
    assert_eq!(&tx.copied_annotation_ids()[..2], seen.as_slice());
    assert_eq!(tx.copied_annotation_ids().len(), 3);
}

#[test]
fn an_id_that_the_transaction_did_not_make_cannot_be_noted_as_a_copy() {
    let s = marked();
    let mut tx = s.begin();
    // Nothing made yet, so no id is in the range.
    assert_eq!(
        tx.note_copied_annotation_id(13).unwrap_err().code(),
        ErrorCode::Internal
    );
    let made = many(&mut tx, 2);
    for id in [12, made[1] + 1, 0, u32::MAX] {
        let e = tx.note_copied_annotation_id(id).unwrap_err();
        assert_eq!(e.code(), ErrorCode::Internal, "{id}");
        assert!(e.message().contains(&id.to_string()), "{e}");
    }
    assert!(
        tx.copied_annotation_ids().is_empty(),
        "a refused note changes nothing"
    );
    tx.note_copied_annotation_id(made[0]).unwrap();
    assert_eq!(tx.copied_annotation_ids(), [made[0]]);
}

#[test]
fn notes_of_copies_change_nothing_about_the_commit_and_go_with_the_transaction() {
    let s = marked();
    let run = |note: bool| {
        let mut tx = s.begin();
        let made = many(&mut tx, 3);
        if note {
            tx.note_copied_annotation_id(made[1]).unwrap();
        }
        for id in &made {
            write_bookmark(&mut tx, *id);
        }
        let (next, committed) = tx.commit().unwrap();
        let exported = next.export().unwrap();
        (
            next.base_token(),
            exported.head,
            exported.refs,
            committed.minted.len(),
        )
    };
    assert_eq!(
        run(true),
        run(false),
        "the version, the lineage, the bytes and the refs are the same"
    );
    // A discarded transaction leaves nothing for the next one.
    let mut tx = s.begin();
    let made = many(&mut tx, 2);
    tx.note_copied_annotation_id(made[0]).unwrap();
    tx.discard();
    assert!(s.begin().copied_annotation_ids().is_empty());
}
