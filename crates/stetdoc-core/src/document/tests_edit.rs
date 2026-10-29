//! `Document::edit` end to end: a batch in, a receipt and a changed document out. The fixture and the helpers are here, the
//! operations in `tests_edit_ops.rs`, and `accept` and `reject` in `tests_edit_review.rs`.

use super::*;
use crate::api::batch::Batch;
use crate::api::edit::Edit;
use crate::api::limits::Limits;
use crate::api::options::{OffsetUnit, Quotes};
use crate::api::receipt::Receipt;
use crate::api::stamp::Stamp;
use crate::api::view::Mode;
use crate::refs::Claim;

/// The bytes of the fixture package, which the testkit's conformance suite shares: ten paragraphs `p1` to `p10` and a table
/// `t1` with rows `r1` and `r2`.
pub(super) fn rich_package() -> Vec<u8> {
    stetdoc_testkit::fixtures::contract::package()
}

/// An unedited document of the fixture.
pub(super) fn rich() -> Document {
    Document::open(rich_package(), &OpenOptions::default()).unwrap()
}

pub(super) fn stamp() -> Stamp {
    Stamp::new("Clauseo", "2026-10-06T10:00:00")
}

/// A batch of JSON operations against the document's current base.
pub(super) fn batch(doc: &Document, ops: &str) -> Batch {
    let json = format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base());
    Batch::parse(&json, &Limits::default()).unwrap()
}

/// Applies operations as Clauseo, tracked.
pub(super) fn run(doc: &mut Document, ops: &str) -> Receipt {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, stamp())).unwrap()
}

/// The error of a batch that must fail.
pub(super) fn fails(doc: &mut Document, ops: &str) -> StetError {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, stamp())).err().unwrap()
}

/// The text of a paragraph in a mode.
pub(super) fn text(doc: &Document, token: &str, mode: Mode) -> String {
    let key = doc.state.resolve(Claim::Implied, token).unwrap();
    let view = doc.state.parts();
    let part = view.id(key.part()).unwrap();
    let story = view.story(part).unwrap();
    let tree = view.tree(part).unwrap();
    let index = story.index_of(key.node()).unwrap();
    story
        .paragraph(tree, index)
        .unwrap()
        .map()
        .text(mode)
        .into_owned()
}

pub(super) fn final_text(doc: &Document, token: &str) -> String {
    text(doc, token, Mode::Final)
}

#[test]
fn a_replace_is_tracked_and_the_receipt_says_what_it_wrote() {
    let mut doc = rich();
    let before = doc.base();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
    assert_eq!(
        text(&doc, "p4", Mode::Original),
        "Payment is due within thirty days."
    );
    assert!(!receipt.dry_run && receipt.tracked);
    assert_eq!(receipt.stamp.author, "Clauseo");
    assert_eq!(receipt.stamp.date, "2026-10-06T10:00:00Z");
    assert_ne!(receipt.base, before);
    assert_eq!(receipt.base, doc.base());
    let op = &receipt.operations[0];
    assert_eq!((op.index, op.op.as_str(), op.changed), (0, "replace", true));
    assert_eq!(op.blocks, ["p4"]);
    let kinds: Vec<_> = op.revisions.iter().map(|r| r.kind).collect();
    assert_eq!(
        kinds,
        [
            crate::api::revision::ChangeKind::Deletion,
            crate::api::revision::ChangeKind::Insertion
        ]
    );
    assert_eq!(receipt.revisions.len(), 1);
    assert_eq!(receipt.revisions[0].author, "Clauseo");
    assert_eq!(receipt.blocks.len(), 1);
    assert_eq!(receipt.blocks[0].r#ref.as_deref(), Some("p4"));
    assert!(
        receipt
            .text
            .starts_with("applied 1 operation, tracked as Clauseo 2026-10-06T10:00:00Z. base ")
    );
    assert!(
        receipt.text.contains("0 replace p4: deletion #"),
        "{}",
        receipt.text
    );
    // The revisions are in the document, by author, with the text they hold.
    let revisions = doc.revisions(&RevisionFilter::default()).unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].author.as_deref(), Some("Clauseo"));
    assert_eq!(revisions[0].deleted.as_deref(), Some("thirty"));
    assert_eq!(revisions[1].inserted.as_deref(), Some("sixty"));
    assert_eq!(revisions[0].block.as_deref(), Some("p4"));
    assert_eq!(revisions[0].story, "body");
    // And saving says the document was edited.
    let saved = doc.save().unwrap();
    assert!(saved.receipt.edited);
    assert_eq!(saved.receipt.revisions.len(), 2);
}

#[test]
fn a_failed_batch_changes_nothing_and_names_the_operation() {
    let mut doc = rich();
    let before = (doc.base(), doc.export().unwrap(), doc.save().unwrap().bytes);
    let e = fails(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},{"op":"replace","ref":"p5","find":"French","with":"x"}"#,
    );
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert_eq!(e.op_index(), Some(1));
    assert_eq!(
        (doc.base(), doc.export().unwrap(), doc.save().unwrap().bytes),
        before
    );
    // A ref that does not exist, and a batch written against another document.
    let e = fails(&mut doc, r#"{"op":"replace","ref":"p99","with":"x"}"#);
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert_eq!(e.op_index(), Some(0));
    let other = crate::document::tests::opened();
    let stale = Batch::parse(
        &format!(
            r#"{{"base":"{}","ops":[{{"op":"delete","ref":"p1"}}]}}"#,
            other.base()
        ),
        &Limits::default(),
    )
    .unwrap();
    let e = doc.edit(Edit::ops(stale, stamp())).err().unwrap();
    assert_eq!(e.code(), ErrorCode::StaleRef, "{e}");
    assert_eq!(doc.base(), before.0);
}

#[test]
fn a_dry_run_is_the_real_edit_without_its_effect() {
    // H14, for what this step builds: the receipts differ in `dry_run`, in `base` (the current one) and in the refs of new blocks.
    let ops = r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},
        {"op":"insert","ref":"p5","place":"after","paragraphs":["One new paragraph.","Another."]},
        {"op":"delete","ref":"p1","find":"and shall pay on demand"}"#;
    let mut dry = rich();
    let mut real = rich();
    let current = dry.base();
    let state_before = (dry.export().unwrap(), dry.save().unwrap().bytes);
    let b = batch(&dry, ops);
    let dry_receipt = dry.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    assert_eq!(
        (dry.export().unwrap(), dry.save().unwrap().bytes),
        state_before,
        "a dry run leaves the document as it was"
    );
    assert_eq!(dry.base(), current);
    let real_receipt = run(&mut real, ops);
    assert!(dry_receipt.dry_run && !real_receipt.dry_run);
    assert_eq!(dry_receipt.base, current);
    assert_ne!(real_receipt.base, current);
    assert_eq!(
        dry_receipt.added,
        Vec::<String>::new(),
        "a dry run issues no ref"
    );
    assert_eq!(real_receipt.added.len(), 2);
    assert_eq!(dry_receipt.added_count, real_receipt.added_count);
    assert_eq!(dry_receipt.added_count, 2);
    for (d, r) in dry_receipt.operations.iter().zip(&real_receipt.operations) {
        assert_eq!(
            (d.index, &d.op, d.changed, &d.blocks),
            (r.index, &r.op, r.changed, &r.blocks)
        );
        assert_eq!(d.added_count, r.added_count);
        assert!(d.added.is_empty());
        assert_eq!(d.revisions.len(), r.revisions.len());
        assert_eq!(d.warnings, r.warnings);
    }
    assert_eq!(dry_receipt.revisions.len(), real_receipt.revisions.len());
    assert_eq!(dry_receipt.blocks.len(), real_receipt.blocks.len());
    for (d, r) in dry_receipt.blocks.iter().zip(&real_receipt.blocks) {
        match (&d.r#ref, &r.r#ref) {
            (Some(a), Some(b)) => assert_eq!(a, b),
            (None, Some(_)) => assert!(d.anchor.is_some(), "a new block shows its anchor"),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(dry_receipt.warnings, real_receipt.warnings);
    assert!(
        dry_receipt
            .text
            .starts_with("dry run: nothing was applied."),
        "{}",
        dry_receipt.text
    );
    // A dry run fails exactly when the real edit fails, with the same code and index.
    let bad =
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},{"op":"delete","ref":"p77"}"#;
    let (mut fresh_dry, mut fresh_real) = (rich(), rich());
    let b = batch(&fresh_dry, bad);
    let e = fresh_dry
        .edit(Edit::ops(b, stamp()).dry_run())
        .err()
        .unwrap();
    let f = fails(&mut fresh_real, bad);
    assert_eq!((e.code(), e.op_index()), (f.code(), f.op_index()));
}

#[test]
fn a_new_block_that_takes_an_older_ref_is_not_listed_as_added() {
    // Another author inserts a paragraph after p10, the last of the body, which marks p10's paragraph mark as inserted. A
    // split of p10 gives its second half that mark, and rejecting the insertion joins the second half into the inserted
    // paragraph, whose ref `p11` the joined block keeps. That ref existed before the batch: no operation lists it as added,
    // and a dry run issues none.
    let prelude =
        r#"{"op":"insert","ref":"p10","place":"after","paragraphs":["Inserted by another."]}"#;
    let ops = r#"{"op":"split","ref":"p10","at":4},{"op":"reject","author":"Other"}"#;
    let open = || {
        let mut doc = rich();
        let b = batch(&doc, prelude);
        doc.edit(Edit::ops(b, Stamp::new("Other", "2026-10-05T10:00:00")))
            .unwrap();
        doc
    };
    let (mut dry, mut real) = (open(), open());
    let b = batch(&dry, ops);
    let dry_receipt = dry.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    let real_receipt = run(&mut real, ops);
    assert_eq!(dry_receipt.operations[0].added_count, 1);
    for (d, r) in dry_receipt.operations.iter().zip(&real_receipt.operations) {
        assert_eq!(d.added, Vec::<String>::new(), "{}", dry_receipt.text);
        assert_eq!(d.added_count, r.added_count);
        assert!(
            r.added.iter().all(|a| real_receipt.added.contains(a)),
            "an operation lists only minted refs: {:?} against {:?}",
            r.added,
            real_receipt.added
        );
    }
}

#[test]
fn an_untracked_edit_writes_no_revision() {
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let receipt = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(!receipt.tracked);
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
    assert_eq!(
        text(&doc, "p4", Mode::Original),
        "Payment is due within sixty days."
    );
    assert!(
        doc.revisions(&RevisionFilter::default())
            .unwrap()
            .is_empty()
    );
    assert!(receipt.revisions.is_empty());
    assert!(receipt.operations[0].revisions.is_empty());
    assert!(receipt.text.contains("untracked"), "{}", receipt.text);
}

#[test]
fn an_edited_document_exports_and_resumes_to_the_same_document() {
    // L4 through an edit: the original is kept at the first edit, so the complement is the edited document's.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let wc = doc.export().unwrap();
    let manifest = wc.manifest().unwrap();
    assert_eq!(manifest.generation, 1);
    assert!(
        manifest.head.is_some(),
        "the head differs from the original now"
    );
    let back = Document::resume(&wc, &OpenOptions::default()).unwrap();
    assert_eq!(back.base(), doc.base());
    assert_eq!(back.export().unwrap(), wc);
    assert_eq!(back.save().unwrap().bytes, doc.save().unwrap().bytes);
    // The saved original is the input, exactly.
    assert_eq!(
        wc.complement.entry("original.docx").unwrap().unwrap(),
        rich_package()
    );
    // A second edit after the resume gives what an unbroken session gives.
    let mut resumed = back;
    let ops = r#"{"op":"replace","ref":"p5","find":"English","with":"Scots"}"#;
    let a = run(&mut doc, ops);
    let b = run(&mut resumed, ops);
    assert_eq!(a.base, b.base);
    assert_eq!(a.text, b.text);
    assert_eq!(resumed.export().unwrap(), doc.export().unwrap());
}

#[test]
fn the_stamp_is_checked_before_anything_is_staged() {
    let mut doc = rich();
    let b = batch(&doc, r#"{"op":"delete","ref":"p4"}"#);
    for bad in [
        Stamp::new("", "2026-10-06T10:00:00"),
        Stamp::new("Clauseo", "yesterday"),
        Stamp::new("x\u{7}", "2026-10-06T10:00:00"),
    ] {
        let e = doc.edit(Edit::ops(b.clone(), bad)).err().unwrap();
        assert_eq!(e.code(), ErrorCode::InvalidPayload);
    }
    // An edit that did not name its operations right is refused before it is applied.
    let empty = Batch::new(doc.base(), Vec::new());
    assert_eq!(
        doc.edit(Edit::ops(empty, stamp())).err().unwrap().code(),
        ErrorCode::InvalidPayload
    );
    let _ = (Quotes::Match, OffsetUnit::Scalar);
}

/// A document of `n` plain paragraphs, "Paragraph 1" and so on.
fn many(n: usize) -> Document {
    let body: String = (1..=n)
        .map(|i| format!("<w:p><w:r><w:t>Paragraph {i}</w:t></w:r></w:p>"))
        .collect();
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(&body)
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

#[test]
fn a_receipt_shows_two_hundred_blocks_at_most_and_counts_the_rest() {
    let ops: Vec<String> = (1..=250)
        .map(|i| format!(r#"{{"op":"replace","ref":"p{i}","find":"Paragraph","with":"Clause"}}"#))
        .collect();
    let ops = ops.join(",");
    let mut doc = many(250);
    let b = batch(&doc, &ops);
    let dry = doc.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    let receipt = run(&mut doc, &ops);
    for r in [&dry, &receipt] {
        assert_eq!((r.blocks.len(), r.blocks_omitted), (200, 50));
        assert_eq!(r.operations.len(), 250);
        assert!(r.text.len() <= super::receipt_text::TEXT_CAP, "{}", r.text);
    }
    // In document order, from the first block, and every operation is still counted whole.
    assert_eq!(receipt.blocks[0].r#ref.as_deref(), Some("p1"));
    assert_eq!(receipt.blocks[199].r#ref.as_deref(), Some("p200"));
    assert!(receipt.operations.iter().all(|o| o.changed));
    assert_eq!(
        receipt.revisions[0]
            .counts
            .iter()
            .map(|(_, n)| *n)
            .sum::<u32>(),
        500
    );
}

#[test]
fn the_text_of_a_receipt_for_hundreds_of_operations_is_capped_and_says_what_it_left_out() {
    let ops: Vec<String> = (1..=300)
        .map(|i| format!(r#"{{"op":"replace","ref":"p{i}","find":"Paragraph","with":"Clause"}}"#))
        .collect();
    let ops = ops.join(",");
    let mut doc = many(300);
    let b = batch(&doc, &ops);
    let dry = doc.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    let receipt = run(&mut doc, &ops);
    for r in [&dry, &receipt] {
        let text = &r.text;
        assert!(
            text.len() <= super::receipt_text::TEXT_CAP,
            "{} bytes",
            text.len()
        );
        // The head line and its base, the first operation and the first block are there.
        let head = text.lines().next().unwrap();
        assert!(
            head.contains("300 operations") && head.ends_with(&format!("base {}", r.base)),
            "{head}"
        );
        assert!(text.contains("\n0 replace p1: "), "{text}");
        assert!(text.contains("\np1 {--Paragraph--}"), "{text}");
        // What it left out, how much, and where it is.
        let listed = text.lines().filter(|l| l.contains(" replace p")).count();
        let more = format!("\nand {} more operations, not listed here", 300 - listed);
        assert!(text.contains(&more), "{text}");
        assert!(
            text.contains(&format!(
                "operations {listed} to 299 are in the JSON receipt (--json)"
            )),
            "{text}"
        );
        assert!(text.contains("`stetdoc view FILE --revisions`"), "{text}");
        let shown = text
            .lines()
            .filter(|l| l.starts_with('p') && l.contains("{++Clause++}"))
            .count();
        let next = format!("p{}", shown + 1);
        let blocks = format!(
            "\nand {} more blocks not shown here, from {next} on: `stetdoc view FILE --range {next}`",
            300 - shown
        );
        assert!(text.contains(&blocks), "{text}");
        assert!(
            text.contains("`stetdoc view FILE --revisions --context 1` shows every changed block"),
            "{text}"
        );
        // The JSON still has every operation.
        assert_eq!(r.operations.len(), 300);
        // A smaller cap keeps the head and the check line, and says what it left out.
        let short = r.text_within(2_000);
        assert!(short.len() <= 2_000, "{} bytes: {short}", short.len());
        assert_eq!(short.lines().next(), text.lines().next());
        assert!(short.contains("\n0 replace p1: "), "{short}");
        assert!(
            short.contains("more operations, not listed here"),
            "{short}"
        );
        assert!(short.contains("validated "), "{short}");
    }
    // A batch that fits lists every operation and shows every block, and says nothing was left out.
    let mut small = many(5);
    let ops: Vec<String> = (1..=5)
        .map(|i| format!(r#"{{"op":"replace","ref":"p{i}","find":"Paragraph","with":"Clause"}}"#))
        .collect();
    let r = run(&mut small, &ops.join(","));
    assert_eq!(
        r.text.lines().filter(|l| l.contains(" replace p")).count(),
        5
    );
    assert_eq!(r.text.lines().filter(|l| l.starts_with('p')).count(), 5);
    assert!(
        !r.text.contains("not listed here") && !r.text.contains("not shown here"),
        "{}",
        r.text
    );
    assert_eq!(r.text_within(2_000), r.text);
}

#[test]
fn a_warning_about_a_block_the_receipt_shows_names_the_block_and_the_dry_run_agrees() {
    // An element that the reader does not know is skipped with its content: the block is shown, and says that it is not all shown.
    let body = "<w:p><w:r><w:t>Alpha beta gamma</w:t></w:r><w:foo w:x=\"1\"><w:r><w:t>hidden words</w:t></w:r></w:foo></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p>";
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(body)
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    let ops = r#"{"op":"replace","ref":"p1","find":"beta","with":"delta"}"#;
    let mut dry = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
    let b = batch(&dry, ops);
    let dry = dry.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    let mut real = Document::open(bytes, &OpenOptions::default()).unwrap();
    let receipt = run(&mut real, ops);
    for r in [&dry, &receipt] {
        let w = r
            .warnings
            .iter()
            .find(|w| w.code == crate::api::view::WarningCode::ReadAnomaly)
            .expect("a warning");
        assert_eq!(
            (w.block.as_deref(), w.story.as_deref()),
            (Some("p1"), Some("body"))
        );
        assert!(
            r.text
                .contains("warning read_anomaly p1: content exists that is not shown"),
            "{}",
            r.text
        );
        assert!(w.next.as_deref().is_some_and(|n| n.contains("check")));
    }
    assert_eq!(dry.warnings, receipt.warnings);
    // The paragraph that was not changed is not shown, and so says nothing.
    assert!(
        receipt
            .blocks
            .iter()
            .all(|b| b.r#ref.as_deref() == Some("p1"))
    );
}

#[test]
fn a_range_across_paragraphs_compares_its_points_in_document_order() {
    // `start` is in p4 and `end` in p5, so 15 before 4 is a range that runs forward.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"delete","ref":"p4","to":"p5","start":15,"end":4}"#,
    );
    assert_eq!(final_text(&doc, "p4"), "Payment is due ");
    assert_eq!(
        final_text(&doc, "p5"),
        " Agreement is governed by English law."
    );
    // In one paragraph the end may not come before the start, and a range in it is deleted.
    let e = fails(
        &mut doc,
        r#"{"op":"delete","ref":"p1","to":"p1","start":9,"end":4}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    run(
        &mut doc,
        r#"{"op":"delete","ref":"p1","to":"p1","start":4,"end":13}"#,
    );
    assert_eq!(
        final_text(&doc, "p1"),
        "The shall indemnify the Customer, and shall pay on demand."
    );
    // The last paragraph before the first is refused, whatever the offsets.
    let e = fails(
        &mut doc,
        r#"{"op":"delete","ref":"p3","to":"p2","start":1,"end":2}"#,
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
}

#[test]
fn a_ref_that_a_merge_absorbs_is_retired_as_merged_into_the_survivor() {
    let untracked = |doc: &mut Document, ops: &str| {
        let b = batch(doc, ops);
        doc.edit(Edit::ops(b, stamp()).untracked()).unwrap()
    };
    let retired = |r: &Receipt| -> Vec<(String, String)> {
        r.retired
            .iter()
            .map(|x| (x.r#ref.clone(), x.reason.clone()))
            .collect()
    };
    let pairs = |v: &[(&str, &str)]| -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    // p5 goes into p4, and the receipt says so, as a later `stale_ref` does.
    let mut doc = rich();
    let receipt = untracked(&mut doc, r#"{"op":"merge","ref":"p4"}"#);
    assert_eq!(retired(&receipt), pairs(&[("p5", "merged into p4")]));
    assert!(
        receipt.text.contains("retired: p5 (merged into p4)"),
        "{}",
        receipt.text
    );
    let e = fails(&mut doc, r#"{"op":"replace","ref":"p5","with":"x"}"#);
    assert!(e.message().contains("p5 was merged into p4"), "{e}");
    // Two merges into one paragraph: both refs are merged into it.
    let mut doc = rich();
    let receipt = untracked(
        &mut doc,
        r#"{"op":"merge","ref":"p3"},{"op":"merge","ref":"p3"}"#,
    );
    assert_eq!(
        retired(&receipt),
        pairs(&[("p4", "merged into p3"), ("p5", "merged into p3")])
    );
    // A merge whose survivor the batch then deletes: the absorbed ref has the survivor's reason.
    let mut doc = rich();
    let receipt = untracked(
        &mut doc,
        r#"{"op":"merge","ref":"p3"},{"op":"delete","ref":"p3"}"#,
    );
    assert_eq!(
        retired(&receipt),
        pairs(&[("p3", "deleted"), ("p4", "deleted")]),
        "{}",
        receipt.text
    );
    let e = fails(&mut doc, r#"{"op":"replace","ref":"p4","with":"x"}"#);
    assert!(e.message().contains("p4 was deleted"), "{e}");
}
