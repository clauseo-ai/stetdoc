//! Step 4 through `Document`: `check`, `check_pending`, the receipt's validation, and the two nets under the compiler.

use super::tests_edit::*;
use super::tests_tamper::arm;
use super::*;
use crate::api::check::{CheckReport, FindingCode};
use crate::api::edit::Edit;
use crate::refs::Claim;

const FAULTY_BODY: &str = concat!(
    "<w:p><w:r><w:rPr><w:i/><w:b/></w:rPr><w:t>First paragraph with a run property out of order</w:t></w:r></w:p>",
    "<w:p><w:pPr><w:rPr><w:i/><w:b/></w:rPr></w:pPr><w:r><w:t>Second paragraph with a mark property out of order</w:t></w:r></w:p>",
    "<w:p><w:r><w:t> leading space and no preserve</w:t></w:r></w:p>",
    "<w:p><w:r><w:t>Fourth paragraph that is clean</w:t></w:r></w:p>",
    "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid>",
    "<w:tr><w:tc><w:p><w:r><w:rPr><w:i/><w:b/></w:rPr><w:t>cell one</w:t></w:r></w:p></w:tc></w:tr>",
    "<w:tr><w:tc><w:p><w:r><w:t>cell two</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    "<w:p><w:r><w:t>Last paragraph</w:t></w:r></w:p>",
);

/// A document whose first three paragraphs and a cell already have a violation each.
fn faulty() -> Document {
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(FAULTY_BODY)
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

/// The codes in a report, each once, sorted.
fn codes(report: &CheckReport) -> Vec<FindingCode> {
    let mut found: Vec<FindingCode> = report.by_code.iter().map(|(c, _)| *c).collect();
    found.sort();
    found
}

#[test]
fn check_reports_what_is_wrong_and_changes_nothing() {
    let doc = faulty();
    let before = (doc.base(), doc.export().unwrap());
    let report = doc.check();
    assert!(!report.ok);
    assert_eq!((report.violations, report.notes, report.parts), (4, 0, 1));
    let at: Vec<(FindingCode, Option<&str>)> = report
        .findings
        .iter()
        .map(|f| (f.code, f.block.as_deref()))
        .collect();
    assert_eq!(
        at,
        [
            (FindingCode::SchemaNonCanonicalOrder, Some("p1")),
            (FindingCode::SchemaNonCanonicalOrder, Some("p2")),
            (FindingCode::SchemaNonCanonicalOrder, Some("p5")),
            (FindingCode::EdgeSpaceWithoutPreserve, Some("p3")),
        ]
    );
    assert_eq!(
        report.by_code,
        [
            (FindingCode::SchemaNonCanonicalOrder, 3),
            (FindingCode::EdgeSpaceWithoutPreserve, 1)
        ]
    );
    // The same twice, and no byte of the document moved.
    assert_eq!(doc.check(), report);
    assert_eq!((doc.base(), doc.export().unwrap()), before);
    let clean = rich().check();
    assert!(clean.ok && clean.violations == 0 && clean.findings.is_empty());
}

#[test]
fn an_edit_is_never_refused_for_a_problem_the_input_already_had() {
    // Output validity on a small document: every built operation, on the blocks that have a violation, is accepted, and what comes out has no
    // finding that the input lacked. The origins that `revisions` declares are what make a copy of a problem not new.
    let input = codes(&faulty().check());
    let ops = [
        r#"{"op":"replace","ref":"p1","find":"First","with":"Opening"}"#,
        r#"{"op":"replace","ref":"p3","find":"leading","with":"initial"}"#,
        r#"{"op":"split","ref":"p1","at":5}"#,
        r#"{"op":"split","ref":"p2","at":6}"#,
        r#"{"op":"merge","ref":"p1","second":"p2"}"#,
        r#"{"op":"insert","ref":"p2","place":"after","paragraphs":["New one."]}"#,
        r#"{"op":"insert","ref":"p3","place":"before","paragraphs":["New two."]}"#,
        r#"{"op":"delete","ref":"p3"}"#,
        r#"{"op":"delete","ref":"p1","find":"First"}"#,
        r#"{"op":"row","ref":"r1","action":"insert","cells":["new cell"]}"#,
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
        r#"{"op":"replace","ref":"p1","find":"First","with":"Opening"},{"op":"accept","all":true}"#,
        r#"{"op":"replace","ref":"p1","find":"First","with":"Opening"},{"op":"reject","all":true}"#,
    ];
    for op in ops {
        let mut doc = faulty();
        let receipt = doc
            .edit(Edit::ops(batch(&doc, op), stamp()))
            .unwrap_or_else(|e| panic!("{op}: {e}"));
        assert!(
            receipt.validation.units >= 1,
            "{op}: {:?}",
            receipt.validation
        );
        assert_eq!(receipt.validation.parts, 1, "{op}");
        let out = codes(&doc.check());
        assert!(
            out.iter().all(|c| input.contains(c)),
            "{op}: {out:?} has a code that the input lacked"
        );
    }
    // The problems that the edited blocks already had are in the receipt's JSON, so the agent is told they are not its own,
    // and its text counts them on one line that says where the list is, rather than repeating them.
    let mut doc = faulty();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"First","with":"Opening"}"#,
    );
    let total = receipt.validation.carried_total;
    assert!(total >= 1);
    assert_eq!(
        receipt.validation.carried[0].code,
        FindingCode::SchemaNonCanonicalOrder
    );
    assert_eq!(receipt.validation.carried[0].block.as_deref(), Some("p1"));
    let line =
        "already in the input, and no new one was added: `stetdoc check --all` lists them.\n";
    let text = &receipt.text;
    assert!(
        text.contains(&format!("{total} problem")) && text.contains(line),
        "{text}"
    );
    assert!(!text.contains("empty_properties"), "not listed: {text}");
}

#[test]
fn a_clean_document_stays_clean_and_the_receipt_has_nothing_carried() {
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},{"op":"split","ref":"p5","at":4},{"op":"row","ref":"r1","action":"insert","cells":["x","y"]}"#,
    );
    assert!(doc.check().ok);
    assert_eq!(receipt.validation.carried_total, 0);
    assert!(receipt.validation.carried.is_empty() && receipt.validation.notes.is_empty());
    assert!(receipt.validation.units >= 3, "{:?}", receipt.validation);
    assert!(receipt.text.contains("checked "), "{}", receipt.text);
    // An edit that changed nothing checked nothing.
    let none = run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"sixty","with":"sixty"}"#,
    );
    assert_eq!((none.validation.parts, none.validation.units), (0, 0));
}

/// Arms a change that puts an element the schema does not allow inside the paragraph `block`, as a compiler that wrote it wrong
/// would have: `check` must see it, though nothing declared it.
fn arm_a_violation_in(doc: &Document, block: &str) {
    let key = doc.state.resolve(Claim::Implied, block).unwrap();
    arm(move |tx| {
        let tree = tx.part_mut(key.part()).unwrap();
        let ns = tree.node_ref(key.node()).namespace().to_owned();
        let bogus = tree.edit().create_element(&ns, "bogus").unwrap();
        tree.edit().append_child(key.node(), bogus).unwrap();
    });
}

#[test]
fn a_violation_the_edit_adds_is_refused_naming_the_operation_and_the_block() {
    let ops = r#"{"op":"replace","ref":"p1","find":"The Supplier","with":"The Seller"},{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#;
    let mut doc = rich();
    let before = (doc.base(), doc.export().unwrap(), doc.save().unwrap().bytes);
    arm_a_violation_in(&doc, "p4");
    let e = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ValidationFailed, "{e}");
    assert_eq!(e.op_index(), Some(1), "the operation that touched p4: {e}");
    assert!(
        e.message()
            .starts_with("The edit would add 1 violation. p4: "),
        "{e}"
    );
    assert_eq!(
        (doc.base(), doc.export().unwrap(), doc.save().unwrap().bytes),
        before
    );
    // A dry run fails exactly as the real edit does.
    arm_a_violation_in(&doc, "p4");
    let d = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).dry_run())
        .err()
        .unwrap();
    assert_eq!(
        (d.code(), d.op_index(), d.message()),
        (e.code(), e.op_index(), e.message())
    );
    // Without the tampering the same batch is accepted, so the refusal was the violation's and nothing else.
    run(&mut doc, ops);
    assert!(doc.check().ok);
}

#[test]
fn a_refusal_of_an_edit_of_files_names_the_file_and_the_line() {
    let mut doc = rich();
    let before = (doc.base(), doc.save().unwrap().bytes);
    let files = {
        let mut f = doc.files().unwrap();
        f.files[0].1 = f.files[0]
            .1
            .replace("within thirty days", "within sixty days");
        f
    };
    arm_a_violation_in(&doc, "p4");
    let e = doc.edit(Edit::files(files, stamp())).err().unwrap();
    assert_eq!(e.code(), ErrorCode::ValidationFailed, "{e}");
    assert_eq!(e.op_index(), None);
    assert_eq!(e.file(), Some("document.md"));
    assert!(e.line().is_some_and(|l| l > 0));
    assert!(
        e.message()
            .starts_with("p4: The edit would add 1 violation."),
        "{e}"
    );
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
}

#[test]
fn a_violation_in_a_part_that_is_no_block_is_refused_without_an_operation() {
    // The first violation is in the frame of the body, not in a paragraph or a table, so no operation can be named.
    let mut doc = rich();
    arm(|tx| {
        let tree = tx.part_mut("word/document.xml").unwrap();
        let root = tree.root().id();
        let ns = tree.node_ref(root).namespace().to_owned();
        let bogus = tree.edit().create_element(&ns, "bogus").unwrap();
        tree.edit().append_child(root, bogus).unwrap();
    });
    let e = doc
        .edit(Edit::ops(
            batch(
                &doc,
                r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
            ),
            stamp(),
        ))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ValidationFailed, "{e}");
    assert_eq!(e.op_index(), None, "{e}");
}

#[test]
fn the_second_pass_of_the_guard_refuses_a_block_that_changed_and_nobody_declared() {
    let ops = r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#;
    // A change to p5, which the scope (p4) does not hold, that no operation made or declared.
    let key = |doc: &Document, block: &str| doc.state.resolve(Claim::Implied, block).unwrap();
    let retext = |key: crate::wordml::BlockKey| {
        move |tx: &mut crate::state::Tx<'_>| {
            let tree = tx.part_mut(key.part()).unwrap();
            let text = tree
                .node_ref(key.node())
                .child_elements()
                .flat_map(|r| r.child_elements().collect::<Vec<_>>())
                .find(|t| t.local_name() == "t")
                .and_then(|t| t.first_child())
                .map(|c| c.id())
                .unwrap();
            tree.edit().set_text(text, "Quietly changed text.").unwrap();
        }
    };
    let mut doc = rich();
    let before = (doc.base(), doc.save().unwrap().bytes);
    arm(retext(key(&doc, "p5")));
    let e = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message().starts_with("p5 is outside the scope (p4)."),
        "{e}"
    );
    assert_eq!(e.op_index(), None, "no operation declared it: {e}");
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    // The same change inside the scope is no one's business, and without a scope nothing is refused.
    arm(retext(key(&doc, "p4")));
    doc.edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
    let mut doc = rich();
    arm(retext(key(&doc, "p5")));
    doc.edit(Edit::ops(batch(&doc, ops), stamp())).unwrap();
}

#[test]
fn a_table_that_differs_only_in_its_rows_is_covered_by_the_rows() {
    // Inserting a row touches its table, and a scope that names the row and not the table is refused by the first pass, naming it.
    let mut doc = rich();
    let insert = r#"{"op":"row","ref":"r1","action":"insert","cells":["x","y"]}"#;
    let e = doc
        .edit(Edit::ops(batch(&doc, insert), stamp()).scope(vec!["r1".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message().starts_with("t1 is outside the scope (r1)."),
        "{e}"
    );
    assert_eq!(e.op_index(), Some(0));
    // The second pass is for what nobody declared. A row taken out of a table with no word of it, whose own properties are the
    // base's, is covered by the rows, and the same table with its properties changed on the side is not.
    let ops = r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#;
    let table = |doc: &Document| doc.state.resolve(Claim::Implied, "t1").unwrap();
    let mut doc = rich();
    let t = table(&doc);
    arm(move |tx| {
        let tree = tx.part_mut(t.part()).unwrap();
        let rows: Vec<_> = tree
            .node_ref(t.node())
            .child_elements()
            .filter(|c| c.local_name() == "tr")
            .map(|c| c.id())
            .collect();
        assert!(rows.len() > 1, "the fixture's table has rows to spare");
        tree.edit().detach(*rows.last().unwrap()).unwrap();
    });
    doc.edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
    // A row that appears with a paragraph that no operation reported making is stetdoc's own bug.
    let mut doc = rich();
    let before = (doc.base(), doc.save().unwrap().bytes);
    let t = table(&doc);
    arm(move |tx| {
        let tree = tx.part_mut(t.part()).unwrap();
        let ns = tree.node_ref(t.node()).namespace().to_owned();
        let (row, cell, para) = (
            tree.edit().create_element(&ns, "tr").unwrap(),
            tree.edit().create_element(&ns, "tc").unwrap(),
            tree.edit().create_element(&ns, "p").unwrap(),
        );
        tree.edit().append_child(cell, para).unwrap();
        tree.edit().append_child(row, cell).unwrap();
        tree.edit().append_child(t.node(), row).unwrap();
    });
    let e = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::Internal, "{e}");
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    let mut doc = rich();
    let t = table(&doc);
    arm(move |tx| {
        let tree = tx.part_mut(t.part()).unwrap();
        let pr = tree
            .node_ref(t.node())
            .child_elements()
            .find(|c| c.local_name() == "tblPr")
            .map(|c| c.id())
            .unwrap();
        let ns = tree.node_ref(pr).namespace().to_owned();
        let jc = tree.edit().create_element(&ns, "jc").unwrap();
        tree.edit().set_attr(jc, &ns, "val", "center").unwrap();
        tree.edit().append_child(pr, jc).unwrap();
    });
    let e = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message().starts_with("t1 is outside the scope (p4)."),
        "{e}"
    );
    assert_eq!(e.op_index(), None);
}

#[test]
fn nothing_pending_is_no_receipt_and_the_exit_status_is_the_strict_rule() {
    // H13. With nothing pending the receipt is None and the status is 0, and `strict` makes any violation, old ones too, a 5.
    let clean = rich();
    let result = clean.check_pending(None, None, false).unwrap();
    assert!(result.receipt.is_none() && result.existing.is_empty() && result.report.ok);
    assert_eq!((result.strict, result.exit_status), (false, 0));
    assert_eq!(
        clean.check_pending(None, None, true).unwrap().exit_status,
        0
    );
    let faulty = faulty();
    let lax = faulty.check_pending(None, None, false).unwrap();
    assert_eq!((lax.exit_status, lax.report.violations), (0, 4));
    let strict = faulty.check_pending(None, None, true).unwrap();
    assert_eq!((strict.exit_status, strict.strict), (5, true));
    assert_eq!(strict.report, faulty.check());
}

#[test]
fn pending_files_that_add_nothing_exit_0_whatever_the_document_holds() {
    let doc = faulty();
    let same = doc.files().unwrap();
    let result = doc
        .check_pending(Some(same.clone()), Some(stamp()), false)
        .unwrap();
    let receipt = result.receipt.expect("a receipt");
    assert!(receipt.dry_run && receipt.operations.is_empty());
    assert_eq!(result.exit_status, 0);
    assert_eq!(
        result.report.violations, 4,
        "the old violations are in the report and do not fail the check"
    );
    assert_eq!(
        doc.check_pending(Some(same), Some(stamp()), true)
            .unwrap()
            .exit_status,
        5
    );
}

#[test]
fn pending_files_are_a_dry_run_that_never_changes_the_document() {
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p5","find":"English","with":"Scots"}"#,
    );
    let mut pending = doc.files().unwrap();
    pending.files[0].1 = pending.files[0].1.replace(
        "p4 Payment is due within thirty days.\n",
        "p4 Payment is due within sixty days.\nA new line.\n",
    );
    let before = (doc.base(), doc.export().unwrap(), doc.save().unwrap().bytes);
    let result = doc
        .check_pending(Some(pending), Some(stamp()), false)
        .unwrap();
    let receipt = result.receipt.expect("a receipt");
    assert!(receipt.dry_run);
    assert_eq!(
        receipt.base,
        doc.base(),
        "a dry run shows the current token"
    );
    assert_eq!(receipt.operations.len(), 2);
    assert!(
        receipt
            .blocks
            .iter()
            .any(|b| b.r#ref.is_none() && b.anchor.is_some()),
        "a new block has no ref"
    );
    assert_eq!(receipt.added, Vec::<String>::new());
    // The revisions the document already holds are listed by author, apart from the receipt's.
    assert_eq!(result.existing.len(), 1);
    assert_eq!(result.existing[0].author, "Clauseo");
    assert_eq!(result.exit_status, 0);
    assert_eq!(
        (doc.base(), doc.export().unwrap(), doc.save().unwrap().bytes),
        before
    );
    // The receipt is the one the real edit gives, apart from what a dry run changes (H14).
    let mut real = rich();
    run(
        &mut real,
        r#"{"op":"replace","ref":"p5","find":"English","with":"Scots"}"#,
    );
    let mut again = real.files().unwrap();
    again.files[0].1 = again.files[0].1.replace(
        "p4 Payment is due within thirty days.\n",
        "p4 Payment is due within sixty days.\nA new line.\n",
    );
    let applied = real.edit(Edit::files(again, stamp())).unwrap();
    assert_eq!(applied.operations.len(), receipt.operations.len());
    assert_eq!(applied.validation, receipt.validation);
}

#[test]
fn pending_files_need_a_stamp_and_a_violation_they_add_is_the_error() {
    let doc = rich();
    let mut pending = doc.files().unwrap();
    pending.files[0].1 = pending.files[0]
        .1
        .replace("within thirty days", "within sixty days");
    let e = doc
        .check_pending(Some(pending.clone()), None, false)
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.message().contains("author and the date"), "{e}");
    // A violation that the pending edit would add is `validation_failed`, whose class is 5, and not a status.
    arm_a_violation_in(&doc, "p4");
    let e = doc
        .check_pending(Some(pending.clone()), Some(stamp()), false)
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ValidationFailed, "{e}");
    assert_eq!(e.code().exit_class().number(), 5);
    assert_eq!(e.file(), Some("document.md"));
    // Files from another version are stale, as for an edit.
    let mut moved = rich();
    run(
        &mut moved,
        r#"{"op":"replace","ref":"p5","find":"English","with":"Scots"}"#,
    );
    let e = moved
        .check_pending(Some(pending), Some(stamp()), false)
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::StaleFiles);
}

#[test]
fn a_hand_edit_in_a_working_copy_is_checked_without_changing_it() {
    // What the CLI does for `stetdoc check <dir>`: the pending files of the directory, through `check_pending`.
    let doc = rich();
    let mut wc = doc.export().unwrap();
    wc.files[0].1 = wc.files[0]
        .1
        .replace("within thirty days", "within sixty days");
    let pending = wc.pending_files().unwrap().expect("one file is pending");
    let before = doc.export().unwrap();
    let result = doc
        .check_pending(Some(pending), Some(stamp()), true)
        .unwrap();
    assert_eq!(result.exit_status, 0);
    assert_eq!(
        result.receipt.expect("a receipt").operations[0].op,
        "replace"
    );
    assert_eq!(doc.export().unwrap(), before);
}

#[test]
fn the_declarations_of_an_edit_are_made_safe_for_check_before_it_sees_them() {
    use crate::index::testing::{Fixture, p};
    use crate::revisions::outcome::Origin;
    use crate::wordml::BlockKey;
    use std::sync::Arc;
    let theme =
        "<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"/>".to_string();
    let state = Fixture::new(&format!("{}{}", p("one"), p("two")))
        .part("theme", "word/theme/theme1.xml", theme)
        .state();
    let mut tx = state.begin();
    let main = state.parts().id("word/document.xml").unwrap();
    let story = state.parts().story(main).unwrap();
    let node = |i: usize| story.blocks().get(i).map(|e| e.node).unwrap();
    let block = |i: usize| BlockKey::new(Arc::from("word/document.xml"), node(i));
    // Two blocks that the edit created, which the base does not have.
    let (made, nothing) = {
        let tree = tx.part_mut("word/document.xml").unwrap();
        let ns = tree.node_ref(tree.root().id()).namespace().to_owned();
        let key = |id| BlockKey::new(Arc::from("word/document.xml"), id);
        (
            key(tree.edit().create_element(&ns, "p").unwrap()),
            key(tree.edit().create_element(&ns, "p").unwrap()),
        )
    };
    // A block of a part that has no schema.
    let unschemed = BlockKey::new(Arc::from("word/theme/theme1.xml"), node(0));
    let origins = vec![
        // kept as it is: both blocks are in the base
        Origin {
            block: block(1),
            from: vec![block(0)],
        },
        // dropped whole: it names a part with no schema
        Origin {
            block: unschemed.clone(),
            from: vec![block(0)],
        },
        // the source in a part with no schema and the block that was made from nothing are left out, and the others stay: one in
        // the base, and one that an origin of the edit says it made
        Origin {
            block: block(0),
            from: vec![unschemed, nothing, made.clone(), block(1)],
        },
        Origin {
            block: made.clone(),
            from: vec![block(0)],
        },
    ];
    let declared = super::declared::build(state.parts(), tx.parts(), &origins).unwrap();
    assert_eq!(
        declared.origins.len(),
        3,
        "the origin in the part with no schema is dropped"
    );
    assert_eq!(declared.origins[0].from, vec![block(0)]);
    assert_eq!(declared.origins[1].from, vec![made.clone(), block(1)]);
    assert_eq!(declared.origins[2].block, made);
    assert!(declared.foreign.is_empty());
}
