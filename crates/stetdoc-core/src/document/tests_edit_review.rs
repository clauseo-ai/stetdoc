//! `accept` and `reject`, and the scope guard, through `Document::edit`.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::revision::ChangeKind;
use crate::api::stamp::Stamp;
use crate::api::view::Mode;

/// A document with two authors' changes: Clauseo changed p4 and p5, and the base is the one after both.
fn edited() -> Document {
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},
           {"op":"replace","ref":"p5","find":"English","with":"Scots"}"#,
    );
    doc
}

fn pending(doc: &Document) -> usize {
    doc.revisions(&RevisionFilter::default()).unwrap().len()
}

#[test]
fn accept_applies_the_revisions_and_the_receipt_counts_them() {
    let mut doc = edited();
    assert_eq!(pending(&doc), 4);
    let receipt = run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(pending(&doc), 0);
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
    assert_eq!(
        text(&doc, "p4", Mode::Original),
        "Payment is due within sixty days."
    );
    let resolution = receipt.operations[0]
        .resolution
        .as_ref()
        .expect("a resolution");
    assert_eq!(resolution.action, "accept");
    let total: u32 = resolution.resolved.iter().map(|c| c.count).sum();
    assert_eq!(total, 4);
    assert!(resolution.resolved.iter().all(|c| c.author == "Clauseo"));
    assert!(receipt.operations[0].changed);
    assert!(receipt.text.contains("accepted"), "{}", receipt.text);
}

#[test]
fn reject_by_the_name_or_the_code_of_an_author_restores_the_original() {
    for who in ["Clauseo", "C"] {
        let mut doc = edited();
        let receipt = run(&mut doc, &format!(r#"{{"op":"reject","author":"{who}"}}"#));
        assert_eq!(pending(&doc), 0, "{who}");
        assert_eq!(
            final_text(&doc, "p4"),
            "Payment is due within thirty days.",
            "{who}"
        );
        assert_eq!(
            final_text(&doc, "p5"),
            "This Agreement is governed by English law.",
            "{who}"
        );
        assert_eq!(
            receipt.operations[0].resolution.as_ref().unwrap().action,
            "reject"
        );
    }
    // An author that the document does not have, and ids that are not there.
    let mut doc = edited();
    let e = fails(&mut doc, r#"{"op":"reject","author":"Nobody"}"#);
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.message().contains("C (Clauseo)"), "{e}");
    let e = fails(&mut doc, r#"{"op":"accept","ids":[9999]}"#);
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert_eq!(pending(&doc), 4);
}

#[test]
fn a_change_is_resolved_whole_by_any_of_its_ids() {
    let mut doc = edited();
    let revisions = doc.revisions(&RevisionFilter::default()).unwrap();
    let first = revisions
        .iter()
        .find(|r| r.block.as_deref() == Some("p4"))
        .expect("p4 has a change");
    let id = first.ids[0];
    run(&mut doc, &format!(r#"{{"op":"reject","ids":[{id}]}}"#));
    // p4's deletion was rejected, and p5 is untouched.
    assert_eq!(pending(&doc), 3);
    let kinds: Vec<ChangeKind> = doc
        .revisions(&RevisionFilter::default())
        .unwrap()
        .iter()
        .map(|r| r.kind)
        .collect();
    assert!(!kinds.is_empty());
}

#[test]
fn a_batch_may_resolve_between_its_edits() {
    // The edits before and after an accept are separate compilers, and the second sees what the accept did.
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},
           {"op":"accept","all":true},
           {"op":"replace","ref":"p4","find":"sixty","with":"ninety"}"#,
    );
    assert_eq!(final_text(&doc, "p4"), "Payment is due within ninety days.");
    assert_eq!(
        text(&doc, "p4", Mode::Original),
        "Payment is due within sixty days."
    );
    assert_eq!(receipt.operations.len(), 3);
    assert!(receipt.operations[1].resolution.is_some());
    assert!(receipt.operations[2].changed);
}

#[test]
fn accepting_nothing_says_so() {
    let mut doc = rich();
    let before = doc.base();
    let receipt = run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert!(!receipt.operations[0].changed);
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.message.contains("resolved nothing")),
        "{:?}",
        receipt.warnings
    );
    assert_eq!(doc.base(), before);
}

#[test]
fn a_scope_lets_through_what_is_inside_it_and_refuses_the_batch_otherwise() {
    let ops = r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#;
    let mut doc = rich();
    let b = batch(&doc, ops);
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
    // Another paragraph is outside, and the whole batch is refused: the first operation did not happen either.
    let mut doc = rich();
    let before = doc.base();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},
           {"op":"replace","ref":"p5","find":"English","with":"Scots"}"#,
    );
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation);
    assert!(
        e.message().starts_with("p5 is outside the scope (p4)."),
        "{e}"
    );
    assert!(
        e.message()
            .contains("Operation 1 (replace p5) would change it."),
        "{e}"
    );
    assert_eq!(e.op_index(), Some(1));
    assert_eq!(doc.base(), before);
    assert_eq!(final_text(&doc, "p4"), "Payment is due within thirty days.");
}

#[test]
fn a_scope_may_be_a_range_or_a_table_and_covers_what_is_inside() {
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},
           {"op":"replace","ref":"p5","find":"English","with":"Scots"}"#,
    );
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p4..p5".to_string()]))
        .unwrap();
    // A table covers its rows and the paragraphs of its cells.
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p6","with":"changed"},{"op":"row","ref":"r2","action":"delete"}"#,
    );
    doc.edit(Edit::ops(b, stamp()).scope(vec!["t1".to_string()]))
        .unwrap();
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["t1".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation);
    // A row covers the paragraphs of its cells and no other row.
    let mut doc = rich();
    let b = batch(&doc, r#"{"op":"replace","ref":"p8","with":"changed"}"#);
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["r1".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
}

#[test]
fn a_new_block_is_covered_by_its_anchor_and_a_scope_checks_what_an_operation_names() {
    // An insertion after a block in scope is in scope, and one next to a block outside it is not.
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"insert","ref":"p4","place":"after","paragraphs":["New."]}"#,
    );
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"insert","ref":"p5","place":"after","paragraphs":["New."]}"#,
    );
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation);
    // A merge names both paragraphs, a delete through names the ends.
    let mut doc = rich();
    let b = batch(&doc, r#"{"op":"merge","ref":"p4"}"#);
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
}

#[test]
fn a_scope_is_checked_for_what_it_names() {
    let mut doc = rich();
    let b = batch(&doc, r#"{"op":"delete","ref":"p4"}"#);
    for (scope, code) in [
        ("p5..p4", ErrorCode::InvalidPayload),
        ("p99", ErrorCode::NotFound),
        ("banana", ErrorCode::InvalidPayload),
    ] {
        let e = doc
            .edit(Edit::ops(b.clone(), stamp()).scope(vec![scope.to_string()]))
            .err();
        let e = e.unwrap_or_else(|| panic!("{scope} was accepted"));
        assert_eq!(e.code(), code, "{scope}: {e}");
    }
    // Too many entries is a limit.
    let many: Vec<String> = (0..10_001).map(|_| "p4".to_string()).collect();
    let e = doc.edit(Edit::ops(b, stamp()).scope(many)).err().unwrap();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
}

#[test]
fn accept_and_reject_under_a_scope_are_narrowed_to_it() {
    // `all` means the revisions in the scope's blocks.
    let mut doc = edited();
    let b = batch(&doc, r#"{"op":"accept","all":true}"#);
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
    assert_eq!(pending(&doc), 2, "only p5's revisions are left");
    let left = doc.revisions(&RevisionFilter::default()).unwrap();
    assert!(left.iter().all(|r| r.block.as_deref() == Some("p5")));
    // By author the same, and an id outside the scope is refused with its number.
    let mut doc = edited();
    let outside = doc
        .revisions(&RevisionFilter::default())
        .unwrap()
        .iter()
        .find(|r| r.block.as_deref() == Some("p5"))
        .map(|r| r.ids[0])
        .expect("a change in p5");
    let b = batch(&doc, &format!(r#"{{"op":"accept","ids":[{outside}]}}"#));
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation);
    assert!(
        e.message()
            .contains(&format!("revision #{outside} is not in the scope (p4)")),
        "{e}"
    );
    assert_eq!(pending(&doc), 4);
    let mut doc = edited();
    let b = batch(&doc, r#"{"op":"reject","author":"Clauseo"}"#);
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p5".to_string()]))
        .unwrap();
    assert_eq!(
        final_text(&doc, "p5"),
        "This Agreement is governed by English law."
    );
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
}

#[test]
fn rejecting_a_split_that_left_a_table_alone_carries_the_problem_of_the_table() {
    // A table inside a paragraph, which the schema does not allow, so the input has that problem. A tracked split at the end
    // of the paragraph's text leaves the table alone in a new paragraph, and a second one splits the text. Rejecting them
    // joins the table back, and nothing else of its paragraph: the problem was there before, so it is carried, not new
    // (tdf111550, seed 271828, batch 32).
    let body = r#"<w:p><w:r><w:t>outer text</w:t></w:r><w:tbl><w:tr><w:tc><w:p><w:r><w:t>inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:p><w:p><w:r><w:t>After.</w:t></w:r></w:p>"#;
    let mut doc = Document::open(
        crate::threads::tests_support::Pkg::new(body).bytes(),
        &OpenOptions::default(),
    )
    .unwrap();
    let before = doc.check();
    assert!(!before.ok, "{before:?}");
    run(
        &mut doc,
        r#"{"op":"split","ref":"p1","at":10},{"op":"split","ref":"p1","at":5}"#,
    );
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(doc.check().by_code, before.by_code);
}

#[test]
fn a_join_carries_the_problems_the_second_paragraph_had() {
    // The second paragraph has a problem of its own (a run property with a value the schema does not allow). Accepting the
    // deletion of the first paragraph's mark joins the two, and the problem moves into the paragraph that survives: it was
    // there before, so it is carried, not new.
    let body = r#"<w:p><w:pPr><w:rPr><w:del w:id="1" w:author="Clauseo" w:date="2026-10-06T10:00:00Z"/></w:rPr></w:pPr><w:r><w:t>First </w:t></w:r></w:p><w:p><w:r><w:rPr><w:sz w:val="big"/></w:rPr><w:t>second.</w:t></w:r></w:p><w:p><w:r><w:t>Third.</w:t></w:r></w:p>"#;
    let mut doc = Document::open(
        crate::threads::tests_support::Pkg::new(body).bytes(),
        &OpenOptions::default(),
    )
    .unwrap();
    let before = doc.check();
    assert!(!before.ok, "{before:?}");
    let receipt = run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert!(receipt.operations[0].changed);
    let after = doc.check();
    assert_eq!(after.by_code, before.by_code, "{after:?}");
    // The same join made by `merge` and accepted later carries it too.
    let body = r#"<w:p><w:r><w:t>First </w:t></w:r></w:p><w:p><w:r><w:rPr><w:sz w:val="big"/></w:rPr><w:t>second.</w:t></w:r></w:p>"#;
    let mut doc = Document::open(
        crate::threads::tests_support::Pkg::new(body).bytes(),
        &OpenOptions::default(),
    )
    .unwrap();
    let before = doc.check();
    run(&mut doc, r#"{"op":"merge","ref":"p1"}"#);
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(doc.check().by_code, before.by_code);
    // And a deletion across paragraphs, whose last paragraph's rest joins the first.
    // The middle paragraph holds a content control, which a deletion keeps; the last one a field, which it keeps too.
    let field = r#"<w:r><w:fldChar w:fldCharType="begin"><w:ffData><w:name w:val="a name far longer than twenty"/><w:enabled/></w:ffData></w:fldChar></w:r><w:r><w:instrText xml:space="preserve"> FORMCHECKBOX </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;
    let control = r#"<w:sdt><w:sdtPr><w:id w:val="5"/></w:sdtPr><w:sdtContent><w:r><w:t>box</w:t></w:r></w:sdtContent></w:sdt>"#;
    let body = format!(
        r#"<w:p><w:r><w:t>First one.</w:t></w:r></w:p><w:p>{control}<w:r><w:t>Middle.</w:t></w:r></w:p><w:p>{control}<w:r><w:t>Another.</w:t></w:r></w:p><w:p>{field}<w:r><w:t>Last</w:t></w:r><w:r><w:t xml:space="preserve"> rest.</w:t></w:r></w:p>"#
    );
    let mut doc = Document::open(
        crate::threads::tests_support::Pkg::new(&body).bytes(),
        &OpenOptions::default(),
    )
    .unwrap();
    let before = doc.check();
    assert!(!before.ok, "{before:?}");
    run(
        &mut doc,
        r#"{"op":"delete","ref":"p1","to":"p4","start":4,"end":5}"#,
    );
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(doc.check().by_code, before.by_code);
}

#[test]
fn a_footer_that_an_accept_takes_out_of_the_document_is_never_an_internal_error() {
    // n830205 (seed 9087444740165966494): another author deleted the paragraph that holds the only section break naming
    // footer 2. Accepting that deletion takes the footer out of the document's stories. An operation after the accept that
    // names a paragraph of the footer is a stale ref; one before it is applied, and the commit retires the footer's refs.
    let body = r#"<w:p><w:r><w:t>Kept.</w:t></w:r></w:p><w:p><w:pPr><w:rPr><w:del w:id="1" w:author="JimBob" w:date="2013-07-17T12:32:00Z"/></w:rPr><w:sectPr><w:footerReference w:type="default" r:id="rIdF2"/></w:sectPr></w:pPr><w:del w:id="2" w:author="JimBob" w:date="2013-07-17T12:32:00Z"><w:r><w:delText>Gone.</w:delText></w:r></w:del></w:p><w:p><w:r><w:t>After.</w:t></w:r></w:p>"#;
    let open = || {
        let fixture = crate::index::testing::Fixture::new(body)
            .footer(1, "<w:p><w:r><w:t>Page footer</w:t></w:r></w:p>")
            .footer(2, "<w:p><w:r><w:t>Someone</w:t></w:r></w:p>")
            .sect(r#"<w:footerReference w:type="default" r:id="rIdF1"/>"#);
        Document::open(fixture.bytes(), &OpenOptions::default()).unwrap()
    };
    let files = open().files().unwrap();
    let footer = files
        .files
        .iter()
        .flat_map(|(_, text)| text.lines())
        .find(|l| l.contains("Someone"))
        .and_then(|l| l.split(' ').next())
        .unwrap()
        .to_string();
    let insert =
        format!(r#"{{"op":"insert","ref":"{footer}","place":"before","paragraphs":["x"]}}"#);
    let accept = r#"{"op":"accept","author":"JimBob"}"#;
    let e = fails(&mut open(), &format!("{accept},{insert}"));
    assert_eq!(
        (e.code(), e.op_index()),
        (ErrorCode::StaleRef, Some(1)),
        "{e}"
    );
    for tracked in [true, false] {
        let mut doc = open();
        let b = batch(&doc, &format!("{insert},{accept}"));
        let edit = Edit::ops(b, stamp());
        let receipt = doc
            .edit(if tracked { edit } else { edit.untracked() })
            .unwrap();
        assert!(
            receipt.retired.iter().any(|r| r.r#ref == footer),
            "{}",
            receipt.text
        );
    }
}

/// A document of one body, from the testkit's fixture builder.
fn of_body(body: &str) -> Document {
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(body)
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

/// Applies operations, tracked, as another author.
fn run_as(doc: &mut Document, author: &str, ops: &str) {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, Stamp::new(author, "2026-10-06T10:00:00")))
        .unwrap();
}

/// The codes of the violations a document has, each once, sorted.
fn violations(doc: &Document) -> Vec<crate::api::check::FindingCode> {
    let mut found: Vec<_> = doc
        .check()
        .by_code
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| c.class() == crate::api::check::Class::Violation)
        .collect();
    found.sort();
    found
}

#[test]
fn a_marker_that_an_accepted_mark_leaves_between_blocks_keeps_the_problem_it_had() {
    // The move's range start has no `w:date`, which the schema requires: a problem of the input. Accepting the deletion of
    // the mark of the paragraph that holds only the start puts the start between the blocks, in the body, and the body
    // must carry the problem of the paragraph that went.
    let mut doc = of_body(concat!(
        "<w:p><w:r><w:t>Intro</w:t></w:r></w:p>",
        r#"<w:p><w:pPr><w:rPr><w:del w:id="1" w:author="B" w:date="2020-01-01T00:00:00Z"/></w:rPr></w:pPr>"#,
        r#"<w:moveFromRangeStart w:id="2" w:author="A" w:name="move1"/></w:p>"#,
        r#"<w:p><w:moveFrom w:id="3" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t>Moved</w:t></w:r></w:moveFrom>"#,
        r#"<w:moveFromRangeEnd w:id="2"/></w:p>"#,
        r#"<w:p><w:moveToRangeStart w:id="4" w:author="A" w:date="2020-01-01T00:00:00Z" w:name="move1"/>"#,
        r#"<w:moveTo w:id="5" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t>Moved</w:t></w:r></w:moveTo>"#,
        r#"<w:moveToRangeEnd w:id="4"/></w:p>"#,
    ));
    let before = violations(&doc);
    assert!(!before.is_empty());
    run(&mut doc, r#"{"op":"accept","author":"B"}"#);
    assert_eq!(violations(&doc), before);
}

#[test]
fn accepting_a_table_deletion_keeps_a_comment_s_start_before_its_end() {
    // The comment's start is in the paragraph and its end inside another author's insertion: the markers that the table's
    // removal keeps go to the next paragraph in document order.
    let mut doc = of_body(concat!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid>"#,
        "<w:tr><w:tc><w:p><w:r><w:t>Table</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
        "<w:p><w:r><w:t>After</w:t></w:r></w:p>",
    ));
    run_as(
        &mut doc,
        "Reviewer",
        r#"{"op":"replace","ref":"p1","start":2,"end":5,"with":"days"},{"op":"comment","ref":"p1","text":"A note."}"#,
    );
    let before = violations(&doc);
    run(&mut doc, r#"{"op":"delete","ref":"t1"}"#);
    run(&mut doc, r#"{"op":"accept","author":"Clauseo"}"#);
    assert_eq!(violations(&doc), before);
    let files = doc.files().unwrap();
    assert!(
        files.files.iter().any(|(_, text)| text.contains("After")),
        "the paragraph after the table is there"
    );
}

#[test]
fn accepting_a_move_removes_a_deletion_that_held_only_its_range_marker() {
    // A tracked delete over a moved run and an insertion after it wraps the range end alone in a `w:del`; accepting the move removes the marker,
    // and the deletion it leaves with nothing in it goes too.
    let mut doc = of_body(concat!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Run 1. </w:t></w:r>",
        r#"<w:moveToRangeStart w:id="0" w:author="A" w:date="2020-01-01T00:00:00Z" w:name="move1"/>"#,
        r#"<w:moveTo w:id="1" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t xml:space="preserve">Run 2. </w:t></w:r></w:moveTo>"#,
        r#"<w:moveToRangeEnd w:id="0"/>"#,
        r#"<w:ins w:id="2" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t xml:space="preserve">Run 4. </w:t></w:r></w:ins>"#,
        r#"<w:moveFromRangeStart w:id="5" w:author="A" w:date="2020-01-01T00:00:00Z" w:name="move1"/>"#,
        r#"<w:moveFrom w:id="6" w:author="A" w:date="2020-01-01T00:00:00Z"><w:r><w:t>Run 2.</w:t></w:r></w:moveFrom>"#,
        r#"<w:moveFromRangeEnd w:id="5"/></w:p>"#,
    ));
    let before = violations(&doc);
    run(
        &mut doc,
        r#"{"op":"delete","ref":"p1"},{"op":"accept","author":"A"},{"op":"format","ref":"p1","bold":true}"#,
    );
    assert_eq!(violations(&doc), before);
}

#[test]
fn a_split_before_the_text_keeps_the_ref_on_the_text_and_a_reject_gives_back_the_very_paragraph() {
    // A split at offset 0 used to leave the original element empty, with the inserted mark, and move the text to a new one.
    // Rejecting removed the empty element, so the text came back under the new ref and without the paragraph's attributes.
    let body = "<w:p w14:paraId=\"1A2B3C4D\" w:rsidR=\"00AB12CD\"><w:pPr><w:jc w:val=\"both\"/></w:pPr><w:r><w:t>Alpha beta gamma.</w:t></w:r></w:p><w:p><w:r><w:t>Second one.</w:t></w:r></w:p>";
    let xml = stetdoc_testkit::fixtures::parts::Doc::new()
        .root_attrs(" xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\"")
        .body(body)
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&xml),
    )
    .build();
    let main_xml = |doc: &Document| {
        let view = doc.state.parts();
        let id = view.id("word/document.xml").unwrap();
        String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
    };
    for how in [r#""at":0"#, r#""before":"Alpha""#] {
        let mut doc = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
        let before = main_xml(&doc);
        let receipt = run(&mut doc, &format!(r#"{{"op":"split","ref":"p1",{how}}}"#));
        assert_eq!(receipt.added, ["p3"], "{how}: {}", receipt.text);
        assert_eq!(final_text(&doc, "p1"), "Alpha beta gamma.", "{how}");
        assert_eq!(final_text(&doc, "p3"), "", "{how}");
        let after = main_xml(&doc);
        assert!(
            after.find("w:ins ").unwrap() < after.find("1A2B3C4D").unwrap(),
            "{how}: the new paragraph stands before the text: {after}"
        );
        let receipt = run(&mut doc, r#"{"op":"reject","all":true}"#);
        assert_eq!(pending(&doc), 0, "{how}");
        assert_eq!(main_xml(&doc), before, "{how}: {}", receipt.text);
        assert_eq!(final_text(&doc, "p1"), "Alpha beta gamma.", "{how}");
        assert!(
            receipt.retired.iter().any(|r| r.r#ref == "p3"),
            "{how}: {:?}",
            receipt.retired
        );
    }
}
