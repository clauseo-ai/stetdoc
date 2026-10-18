//! The operations through `Document::edit`: what each writes, and what each refuses.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::options::{OffsetUnit, Quotes};
use crate::api::revision::ChangeKind;
use crate::api::view::{Mode, WarningCode};

const P4: &str = "Payment is due within thirty days.";

#[test]
fn replace_finds_one_match_or_says_where_they_all_are() {
    let mut doc = rich();
    // `shall` is in p1 twice.
    let e = fails(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"shall","with":"must"}"#,
    );
    assert_eq!(e.code(), ErrorCode::AmbiguousMatch);
    assert!(
        e.message().contains("matches 2 times in p1")
            && e.message().contains("1: at 13")
            && e.message().contains("[shall]"),
        "{e}"
    );
    assert!(e.next().contains("occurrence"), "{}", e.next());
    // `occurrence` counts from 1.
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"shall","occurrence":2,"with":"must"}"#,
    );
    assert_eq!(
        final_text(&doc, "p1"),
        "The Supplier shall indemnify the Customer, and must pay on demand."
    );
    let e = fails(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"days","occurrence":2,"with":"x"}"#,
    );
    assert_eq!(e.code(), ErrorCode::NotFound);
    // No match, and a match that is only in a deletion, which says whose.
    let e = fails(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"forty","with":"x"}"#,
    );
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(
        e.message().contains("is not in p4") && e.message().contains("Payment is due"),
        "{e}"
    );
    let e = fails(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"shall pay","with":"x"}"#,
    );
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.message().contains("tracked deletion by Clauseo"), "{e}");
    assert!(e.next().contains("Reject that change"), "{}", e.next());
}

#[test]
fn replace_takes_offsets_or_the_whole_paragraph() {
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","start":22,"end":28,"with":"sixty"}"#,
    );
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p5","with":"Scots law governs this Agreement."}"#,
    );
    assert_eq!(final_text(&doc, "p5"), "Scots law governs this Agreement.");
    assert_eq!(
        text(&doc, "p5", Mode::Original),
        "This Agreement is governed by English law."
    );
    // The whole paragraph is a difference by words: the words that did not change are not rewritten.
    assert!(receipt.operations[0].revisions.len() >= 2);
    for (ops, why) in [
        (
            r#"{"op":"replace","ref":"p4","start":3,"end":999,"with":"x"}"#,
            "past the end",
        ),
        (r#"{"op":"replace","ref":"t1","with":"x"}"#, "a table"),
        (r#"{"op":"replace","ref":"r1","with":"x"}"#, "a table row"),
    ] {
        let e = fails(&mut doc, ops);
        assert!(e.message().contains(why), "{ops}: {e}");
    }
    assert_eq!(
        fails(&mut doc, r#"{"op":"replace","ref":"t1","with":"x"}"#).code(),
        ErrorCode::CrossBlock
    );
    assert_eq!(
        fails(
            &mut doc,
            r#"{"op":"replace","ref":"p4","start":3,"end":999,"with":"x"}"#
        )
        .code(),
        ErrorCode::InvalidPayload
    );
}

#[test]
fn a_replace_that_changes_nothing_says_so_and_does_not_move_the_version() {
    let mut doc = rich();
    let before = doc.base();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"thirty"}"#,
    );
    assert!(!receipt.operations[0].changed);
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::NoChange),
        "{:?}",
        receipt.warnings
    );
    assert!(receipt.text.contains("no change"), "{}", receipt.text);
    assert_eq!(doc.base(), before);
    assert_eq!(receipt.base, before);
    assert!(
        doc.revisions(&RevisionFilter::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn offsets_count_utf16_units_when_the_edit_says_so() {
    // p3 is "Emoji \u{1F600} then a tab\tand the end.": the emoji is one scalar value and two UTF-16 units.
    let ops = |start, end| {
        format!(r#"{{"op":"replace","ref":"p3","start":{start},"end":{end},"with":"and"}}"#)
    };
    let mut scalar = rich();
    run(&mut scalar, &ops(8, 12));
    assert_eq!(
        final_text(&scalar, "p3"),
        "Emoji \u{1F600} and a tab\tand the end."
    );
    let mut utf16 = rich();
    let b = batch(&utf16, &ops(9, 13));
    utf16
        .edit(Edit::ops(b, stamp()).offsets(OffsetUnit::Utf16))
        .unwrap();
    assert_eq!(final_text(&utf16, "p3"), final_text(&scalar, "p3"));
    // An offset inside the pair is refused, and so is one past the end, and the tab is one unit and one scalar value.
    let mut doc = rich();
    for bad in [7, 999] {
        let b = batch(&doc, &ops(bad, bad + 1));
        let e = doc
            .edit(Edit::ops(b, stamp()).offsets(OffsetUnit::Utf16))
            .err()
            .unwrap();
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{bad}: {e}");
    }
    let tab = "Emoji \u{1F600} then a tab".chars().count();
    let b = batch(
        &doc,
        &format!(
            r#"{{"op":"replace","ref":"p3","start":{},"end":{},"with":" "}}"#,
            tab + 1 + 1,
            tab + 1 + 1
        ),
    );
    doc.edit(Edit::ops(b, stamp()).offsets(OffsetUnit::Utf16))
        .unwrap();
}

#[test]
fn typed_quotes_follow_the_paragraph_and_say_so() {
    // p2 is curly, and p1 has no quote and sits next to it.
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p2","find":"means any loss","with":"means \"any\" loss"}"#,
    );
    assert_eq!(
        final_text(&doc, "p2"),
        "\u{201C}Losses\u{201D} means \u{201C}any\u{201D} loss, and the Supplier\u{2019}s liability."
    );
    let warning = receipt
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::QuotesCurled)
        .expect("a warning");
    assert_eq!(warning.block.as_deref(), Some("p2"));
    assert_eq!(warning.story.as_deref(), Some("body"));
    // The neighbours decide for a paragraph with no quote of its own.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"shall pay","with":"shall \"pay\""}"#,
    );
    assert_eq!(
        final_text(&doc, "p1"),
        "The Supplier shall indemnify the Customer, and shall \u{201C}pay\u{201D} on demand."
    );
    // The character the text goes after decides too: `'s` typed right after `Supplier` is an apostrophe.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p1","start":12,"end":12,"with":"'s"}"#,
    );
    assert_eq!(
        final_text(&doc, "p1"),
        "The Supplier\u{2019}s shall indemnify the Customer, and shall pay on demand."
    );
    // As typed keeps every character.
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p2","find":"means any loss","with":"means \"any\" loss"}"#,
    );
    let receipt = doc
        .edit(Edit::ops(b, stamp()).quotes(Quotes::AsTyped))
        .unwrap();
    assert!(final_text(&doc, "p2").contains("means \"any\" loss"));
    assert!(
        receipt
            .warnings
            .iter()
            .all(|w| w.code != WarningCode::QuotesCurled)
    );
    // A straight `find` for a curly quote matches, and says it matched loosely.
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p2","find":"Supplier's liability","with":"Customer's liability"}"#,
    );
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::FindMatchedLoosely),
        "{:?}",
        receipt.warnings
    );
    assert!(
        final_text(&doc, "p2").ends_with("the Customer\u{2019}s liability."),
        "{}",
        final_text(&doc, "p2")
    );
}

#[test]
fn the_quote_policy_shapes_only_what_was_typed_and_never_the_text_carried_over() {
    // p2 is curly, and a straight apostrophe is put in it as typed, so the paragraph mixes the two.
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p2","find":"any loss","with":"O'Neill's loss"}"#,
    );
    doc.edit(Edit::ops(b, stamp()).quotes(Quotes::AsTyped))
        .unwrap();
    let mixed = final_text(&doc, "p2");
    assert!(mixed.contains("O'Neill's loss"), "{mixed}");
    // The whole paragraph is written again with its last word changed, as an edit of the files does: the straight
    // apostrophes it carries over stay straight, and only the last word is a change.
    let with = mixed.replace("liability.", "duty.");
    let receipt = run(
        &mut doc,
        &format!(
            r#"{{"op":"replace","ref":"p2","with":{}}}"#,
            serde_json::to_string(&with).unwrap()
        ),
    );
    assert_eq!(final_text(&doc, "p2"), with);
    assert!(
        receipt
            .warnings
            .iter()
            .all(|w| w.code != WarningCode::QuotesCurled),
        "{:?}",
        receipt.warnings
    );
    // A quote typed in the same edit still follows the paragraph.
    let with = with.replace("duty.", "\"duty\".");
    run(
        &mut doc,
        &format!(
            r#"{{"op":"replace","ref":"p2","with":{}}}"#,
            serde_json::to_string(&with).unwrap()
        ),
    );
    assert!(
        final_text(&doc, "p2")
            .ends_with("O'Neill's loss, and the Supplier\u{2019}s \u{201C}duty\u{201D}."),
        "{}",
        final_text(&doc, "p2")
    );
}

#[test]
fn insert_puts_new_paragraphs_in_order_and_the_receipt_gives_their_refs() {
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"insert","ref":"p5","place":"after","paragraphs":["First new.","Second new."]}"#,
    );
    let op = &receipt.operations[0];
    assert_eq!(op.added.len(), 2);
    assert_eq!(op.added_count, 2);
    assert_eq!(receipt.added, op.added);
    assert_eq!(final_text(&doc, &op.added[0]), "First new.");
    assert_eq!(final_text(&doc, &op.added[1]), "Second new.");
    assert_eq!(text(&doc, &op.added[0], Mode::Original), "");
    assert!(op.revisions.iter().all(|r| matches!(
        r.kind,
        ChangeKind::ParagraphInserted | ChangeKind::MarkInserted | ChangeKind::Insertion
    )));
    assert_eq!(receipt.blocks.len(), 2);
    assert!(receipt.blocks.iter().all(|b| b.r#ref.is_some()));
    // Before an anchor, and next to a table.
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"insert","ref":"p4","place":"before","paragraphs":["Before p4."]},
           {"op":"insert","ref":"t1","place":"after","paragraphs":["After the table."]}"#,
    );
    assert_eq!(
        final_text(&doc, &receipt.operations[0].added[0]),
        "Before p4."
    );
    assert_eq!(
        final_text(&doc, &receipt.operations[1].added[0]),
        "After the table."
    );
}

#[test]
fn insert_refuses_a_row_and_a_style_the_document_does_not_have() {
    let mut doc = rich();
    let e = fails(
        &mut doc,
        r#"{"op":"insert","ref":"r1","place":"after","paragraphs":["x"]}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.message().contains("`row`"), "{e}");
    let e = fails(
        &mut doc,
        r#"{"op":"insert","ref":"p4","place":"after","paragraphs":[{"text":"x","style":"Heading 1"}]}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(
        e.message().contains("no paragraph style `Heading 1`"),
        "{e}"
    );
    assert_eq!(e.op_index(), Some(0));
}

#[test]
fn delete_removes_a_paragraph_a_range_a_table_or_the_blocks_through_another() {
    // A paragraph.
    let mut doc = rich();
    let receipt = run(&mut doc, r#"{"op":"delete","ref":"p4"}"#);
    assert_eq!(final_text(&doc, "p4"), "");
    assert_eq!(text(&doc, "p4", Mode::Original), P4);
    assert!(receipt.operations[0].changed);
    // A range of one paragraph, by text.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"delete","ref":"p1","find":", and shall pay on demand"}"#,
    );
    assert_eq!(
        final_text(&doc, "p1"),
        "The Supplier shall indemnify the Customer."
    );
    // A table, whole.
    let mut doc = rich();
    let receipt = run(&mut doc, r#"{"op":"delete","ref":"t1"}"#);
    assert!(receipt.operations[0].changed);
    for cell in ["p6", "p7", "p8", "p9"] {
        assert_eq!(final_text(&doc, cell), "", "{cell}");
    }
    // From the middle of p4 through the middle of p5: the paragraphs join.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"delete","ref":"p4","to":"p5","start":15,"end":15}"#,
    );
    // The tail of p4 and the head of p5 are deleted, and the mark of p4 goes, so the two read as one paragraph.
    assert_eq!(final_text(&doc, "p4"), "Payment is due ");
    assert_eq!(final_text(&doc, "p5"), "is governed by English law.");
    let kinds: Vec<_> = doc
        .revisions(&RevisionFilter::default())
        .unwrap()
        .iter()
        .map(|r| r.kind)
        .collect();
    assert!(kinds.contains(&ChangeKind::MarkDeleted), "{kinds:?}");
    assert!(kinds.contains(&ChangeKind::Deletion), "{kinds:?}");
    // The blocks between go whole, and a range that holds a table is refused.
    let mut doc = rich();
    run(&mut doc, r#"{"op":"delete","ref":"p1","to":"p3"}"#);
    for p in ["p1", "p2", "p3"] {
        assert_eq!(final_text(&doc, p), "", "{p}");
    }
    assert_eq!(final_text(&doc, "p4"), "Payment is due within thirty days.");
    let e = fails(&mut doc, r#"{"op":"delete","ref":"p4","to":"p10"}"#);
    assert_eq!(e.code(), ErrorCode::CrossBlock);
    let e = fails(&mut doc, r#"{"op":"delete","ref":"r1"}"#);
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.message().contains("`row`"), "{e}");
    let e = fails(&mut doc, r#"{"op":"delete","ref":"p5","to":"p4"}"#);
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
}

#[test]
fn split_cuts_at_an_offset_or_before_or_after_some_text() {
    for (ops, head, tail) in [
        (
            r#"{"op":"split","ref":"p4","at":11}"#,
            "Payment is ",
            "due within thirty days.",
        ),
        (
            r#"{"op":"split","ref":"p4","before":"due"}"#,
            "Payment is ",
            "due within thirty days.",
        ),
        (
            r#"{"op":"split","ref":"p4","after":"is "}"#,
            "Payment is ",
            "due within thirty days.",
        ),
    ] {
        let mut doc = rich();
        let receipt = run(&mut doc, ops);
        assert_eq!(final_text(&doc, "p4"), head, "{ops}");
        let added = &receipt.operations[0].added;
        assert_eq!(added.len(), 1, "{ops}");
        assert_eq!(final_text(&doc, &added[0]), tail, "{ops}");
        assert!(
            receipt.operations[0]
                .revisions
                .iter()
                .any(|r| r.kind == ChangeKind::MarkInserted),
            "{ops}"
        );
    }
    let mut doc = rich();
    let e = fails(&mut doc, r#"{"op":"split","ref":"p1","before":"shall"}"#);
    assert_eq!(e.code(), ErrorCode::AmbiguousMatch);
    let e = fails(&mut doc, r#"{"op":"split","ref":"t1","at":1}"#);
    assert_eq!(e.code(), ErrorCode::CrossBlock);
}

#[test]
fn merge_joins_a_paragraph_with_the_next_or_with_the_one_named() {
    let mut doc = rich();
    let receipt = run(&mut doc, r#"{"op":"merge","ref":"p4"}"#);
    assert!(
        receipt.operations[0]
            .revisions
            .iter()
            .any(|r| r.kind == ChangeKind::MarkDeleted)
    );
    assert_eq!(
        final_text(&doc, "p5"),
        "This Agreement is governed by English law."
    );
    let mut named = rich();
    run(&mut named, r#"{"op":"merge","ref":"p4","second":"p5"}"#);
    // A table between is a refusal, and a paragraph with nothing to merge with is.
    let mut doc2 = rich();
    let e = fails(&mut doc2, r#"{"op":"merge","ref":"p5"}"#);
    assert_eq!(e.code(), ErrorCode::CrossBlock, "{e}");
    let e = fails(&mut doc2, r#"{"op":"merge","ref":"p10"}"#);
    assert_eq!(e.code(), ErrorCode::CrossBlock);
    assert!(e.message().contains("no paragraph after it"), "{e}");
    let e = fails(&mut doc2, r#"{"op":"merge","ref":"t1"}"#);
    assert_eq!(e.code(), ErrorCode::CrossBlock);
}

#[test]
fn row_inserts_a_row_with_the_cell_texts_or_deletes_one() {
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"row","ref":"r1","action":"insert","cells":["x1","y1"]}"#,
    );
    let op = &receipt.operations[0];
    assert!(
        op.revisions
            .iter()
            .any(|r| r.kind == ChangeKind::RowInserted),
        "{:?}",
        op.revisions
    );
    // The receipt shows the paragraphs of the new row, which a reader reads.
    let shown: Vec<&str> = receipt.blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(
        shown.iter().any(|t| t.contains("x1")) && shown.iter().any(|t| t.contains("y1")),
        "{shown:?}"
    );
    let mut doc = rich();
    let receipt = run(&mut doc, r#"{"op":"row","ref":"r2","action":"delete"}"#);
    assert!(
        receipt.operations[0]
            .revisions
            .iter()
            .any(|r| r.kind == ChangeKind::RowDeleted)
    );
    let e = fails(&mut doc, r#"{"op":"row","ref":"p4","action":"delete"}"#);
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    let e = fails(
        &mut doc,
        r#"{"op":"row","ref":"r1","action":"delete","cells":["x"]}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
}

#[test]
fn a_batch_cannot_name_a_block_that_an_earlier_operation_made() {
    // H5: refs are minted at commit, so the paragraph a batch inserts has none while the batch runs. The real edit gives it
    // `p11`, and a batch that names `p11` in the same breath is refused.
    let mut real = rich();
    let receipt = run(
        &mut real,
        r#"{"op":"insert","ref":"p5","place":"after","paragraphs":["Made."]}"#,
    );
    assert_eq!(receipt.operations[0].added, ["p11"]);
    let mut doc = rich();
    let before = doc.base();
    let e = fails(
        &mut doc,
        r#"{"op":"insert","ref":"p5","place":"after","paragraphs":["Made."]},{"op":"replace","ref":"p11","with":"x"}"#,
    );
    assert!(
        matches!(e.code(), ErrorCode::NotFound | ErrorCode::StaleRef),
        "{e}"
    );
    assert_eq!(e.op_index(), Some(1));
    assert_eq!(doc.base(), before);
}

#[test]
fn removing_a_block_twice_in_one_batch_is_said_and_a_removed_ref_is_retired() {
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"delete","ref":"p4"},{"op":"delete","ref":"p4"}"#,
    );
    assert!(receipt.operations[0].changed);
    assert!(!receipt.operations[1].changed);
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.message.contains("already deleted")),
        "{:?}",
        receipt.warnings
    );
    // Untracked, a deleted paragraph leaves the document and its ref is retired.
    let mut doc = rich();
    let b = batch(&doc, r#"{"op":"delete","ref":"p4"}"#);
    let receipt = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert_eq!(receipt.retired.len(), 1);
    assert_eq!(receipt.retired[0].r#ref, "p4");
    assert_eq!(receipt.retired[0].reason, "deleted");
    assert!(
        receipt.text.contains("retired: p4 (deleted)"),
        "{}",
        receipt.text
    );
    let e = fails(&mut doc, r#"{"op":"replace","ref":"p4","with":"x"}"#);
    assert_eq!(e.code(), ErrorCode::StaleRef, "{e}");
}
