//! Step 6 through `Document::edit`: the comment operations, `delete-note`, `reconcile`, and the comment rules of the scope guard.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use crate::api::stamp::Stamp;
use crate::api::view::WarningCode;

/// A comment by Clauseo on `ref`, and its id as the receipt gives it.
fn commented(doc: &mut Document, r: &str) -> String {
    let receipt = run(
        doc,
        &format!(r#"{{"op":"comment","ref":"{r}","find":"thirty","text":"Too short."}}"#),
    );
    receipt.operations[0].comment.clone().expect("a comment id")
}

fn scoped(doc: &mut Document, ops: &str, scope: &str) -> Result<Receipt, StetError> {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, stamp()).scope(vec![scope.to_string()]))
}

#[test]
fn a_comment_is_added_and_the_receipt_names_it_so_the_next_operation_can() {
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"comment","ref":"p4","find":"thirty","text":"Too short."}"#,
    );
    let op = &receipt.operations[0];
    assert!(op.changed);
    let id = op.comment.clone().unwrap();
    assert_eq!(receipt.comments.len(), 1);
    let c = &receipt.comments[0];
    assert_eq!(
        (c.action.as_str(), c.id.as_str(), c.author.as_str()),
        ("added", id.as_str(), "Clauseo")
    );
    assert_eq!(c.blocks, ["p4"]);
    assert!(
        receipt.text.contains(&format!("{id} added")),
        "{}",
        receipt.text
    );
    assert!(doc.check().ok, "{:?}", doc.check());
    // Reply, resolve, edit and delete name it.
    let receipt = run(
        &mut doc,
        &format!(r#"{{"op":"reply","comment":"{id}","text":"Agreed."}}"#),
    );
    assert_eq!(receipt.comments[0].action, "replied");
    assert_eq!(receipt.comments[0].thread, id);
    let receipt = run(&mut doc, &format!(r#"{{"op":"resolve","comment":"{id}"}}"#));
    assert_eq!(receipt.comments[0].action, "resolved");
    let receipt = run(&mut doc, &format!(r#"{{"op":"resolve","comment":"{id}"}}"#));
    assert!(!receipt.operations[0].changed);
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::AlreadyResolved)
    );
    let receipt = run(
        &mut doc,
        &format!(r#"{{"op":"edit-comment","comment":"{id}","text":"Far too short."}}"#),
    );
    assert_eq!(receipt.comments[0].action, "edited");
    let receipt = run(
        &mut doc,
        &format!(r#"{{"op":"delete-comment","comment":"{id}"}}"#),
    );
    assert_eq!(receipt.comments[0].action, "deleted");
    assert_eq!(receipt.comments.len(), 2, "the comment and its reply");
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn a_comment_that_names_nothing_is_refused_with_its_code() {
    let mut doc = rich();
    for (ops, code) in [
        (
            r#"{"op":"reply","comment":"c99","text":"x"}"#,
            ErrorCode::NotFound,
        ),
        (
            r#"{"op":"comment","ref":"t1","text":"x"}"#,
            ErrorCode::CrossBlock,
        ),
        (r#"{"op":"delete-note","note":"fn1"}"#, ErrorCode::NotFound),
    ] {
        let e = fails(&mut doc, ops);
        assert_eq!(e.code(), code, "{ops}: {e}");
        assert_eq!(e.op_index(), Some(0), "{ops}");
    }
    // Another author's comment is not Clauseo's to edit.
    let id = commented(&mut doc, "p4");
    let b = batch(
        &doc,
        &format!(r#"{{"op":"edit-comment","comment":"{id}","text":"Mine now."}}"#),
    );
    let e = doc
        .edit(Edit::ops(b, Stamp::new("Ann", "2026-10-06T11:00:00")))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{e}");
}

#[test]
fn under_a_scope_a_comment_operation_is_checked_by_its_anchor() {
    // A comment on a paragraph in scope passes, with its parts and styles as declared side effects, and one outside is refused.
    let mut doc = rich();
    scoped(
        &mut doc,
        r#"{"op":"comment","ref":"p4","text":"Inside."}"#,
        "p4",
    )
    .unwrap();
    let e = scoped(
        &mut doc,
        r#"{"op":"comment","ref":"p5","text":"Outside."}"#,
        "p4",
    )
    .err()
    .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message().starts_with("p5 is outside the scope (p4)."),
        "{e}"
    );
    assert_eq!(e.op_index(), Some(0));
    // `resolve`, `reply`, `edit-comment` and `delete-comment` change no block of their anchor, and are checked by it.
    let mut doc = rich();
    let id = commented(&mut doc, "p4");
    for ops in [
        format!(r#"{{"op":"resolve","comment":"{id}"}}"#),
        format!(r#"{{"op":"reply","comment":"{id}","text":"x"}}"#),
        format!(r#"{{"op":"edit-comment","comment":"{id}","text":"y"}}"#),
        format!(r#"{{"op":"delete-comment","comment":"{id}"}}"#),
    ] {
        let mut copy = Document::open(doc.save().unwrap().bytes, &OpenOptions::default()).unwrap();
        let e = scoped(&mut copy, &ops, "p5").err().unwrap();
        assert_eq!(e.code(), ErrorCode::ScopeViolation, "{ops}: {e}");
        scoped(&mut copy, &ops, "p4").unwrap_or_else(|e| panic!("{ops}: {e}"));
    }
}

#[test]
fn reconcile_runs_after_the_operations_and_its_warnings_reach_the_receipt() {
    // A comment whose text a tracked deletion takes keeps its reference and stays, and `reconcile` says its range is empty.
    let mut doc = rich();
    let id = commented(&mut doc, "p4");
    let receipt = run(
        &mut doc,
        r#"{"op":"delete","ref":"p4","find":"within thirty days"}"#,
    );
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::AnchorEmptied && w.message.contains(&id)),
        "{:?}",
        receipt.warnings
    );
    assert!(receipt.comments.is_empty(), "nothing was removed");
    // An untracked deletion of the paragraph hoists the reference, so the comment survives that too.
    let b = batch(&doc, r#"{"op":"delete","ref":"p4"}"#);
    let receipt = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(receipt.comments.iter().all(|c| c.action != "removed"));
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn delete_note_names_the_note_as_the_view_shows_it_and_owns_its_paragraphs() {
    let open = || {
        Document::open(
            stetdoc_testkit::fixtures::contract::package_rich(),
            &OpenOptions::default(),
        )
        .unwrap()
    };
    let mut doc = open();
    assert!(doc.check().ok, "{:?}", doc.check());
    let receipt = run(&mut doc, r#"{"op":"delete-note","note":"fn1"}"#);
    assert!(receipt.operations[0].changed);
    assert!(
        receipt.operations[0].blocks.contains(&"p5".to_string()),
        "{:?}",
        receipt.operations[0]
    );
    assert!(
        !doc.revisions(&RevisionFilter::default())
            .unwrap()
            .is_empty()
    );
    assert!(doc.check().ok, "{:?}", doc.check());
    // Under a scope the paragraph of the reference must be in it, and the note's own paragraphs need not be.
    let mut doc = open();
    scoped(&mut doc, r#"{"op":"delete-note","note":"fn1"}"#, "p5").unwrap();
    let mut doc = open();
    let e = scoped(&mut doc, r#"{"op":"delete-note","note":"fn1"}"#, "p4")
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    let e = fails(&mut doc, r#"{"op":"delete-note","note":"fn2"}"#);
    assert_eq!(e.code(), ErrorCode::NotFound, "{e}");
}

#[test]
fn a_tracked_delete_note_counts_only_the_ids_it_made_when_the_document_uses_large_ids() {
    // A bookmark with a `w:id` past two billion, as Word writes them: the first id the edit makes is one more. `delete-note`
    // took the ids from 0 to there as its own, and the receipt counted each of them into a map, which took the process past
    // 6 GB (aspose-words-net's DocumentVisitor-compatible features.docx, the nightly laws run, shard 1 of 16).
    let body = r#"<w:p><w:bookmarkStart w:id="2112897428" w:name="far"/><w:r><w:t xml:space="preserve">Video </w:t></w:r><w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteReference w:id="2"/></w:r><w:r><w:t>provides a way.</w:t></w:r><w:bookmarkEnd w:id="2112897428"/></w:p>"#;
    let note = r#"<w:footnote w:id="2"><w:p><w:r><w:rPr><w:rStyle w:val="FootnoteReference"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> A note.</w:t></w:r></w:p></w:footnote>"#;
    let bytes = crate::threads::tests_support::Pkg::new(body)
        .footnotes(note)
        .bytes();
    let mut doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    let receipt = run(&mut doc, r#"{"op":"delete-note","note":"fn1"}"#);
    assert!(receipt.operations[0].changed);
    let ids: Vec<i64> = receipt.operations[0]
        .revisions
        .iter()
        .flat_map(|r| r.ids.iter().copied())
        .collect();
    assert!(
        !ids.is_empty() && ids.iter().all(|id| *id > 2_112_897_428),
        "{ids:?}"
    );
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn under_a_scope_a_comment_declares_the_styles_it_appended_and_nothing_else_of_styles_xml() {
    let rich = || {
        Document::open(
            stetdoc_testkit::fixtures::contract::package_rich(),
            &OpenOptions::default(),
        )
        .unwrap()
    };
    let ops = r#"{"op":"comment","ref":"p4","text":"Inside."}"#;
    // The comment styles are appended to an existing styles part, and that passes.
    let mut doc = rich();
    let receipt = scoped(&mut doc, ops, "p4").unwrap();
    assert_eq!(receipt.comments.len(), 1);
    // Anything else changed in styles.xml on the side is refused, though a comment declared the part.
    let mut doc = rich();
    let before = (doc.base(), doc.save().unwrap().bytes);
    super::tests_tamper::arm(|tx| {
        let tree = tx.part_mut("word/styles.xml").unwrap();
        let style = tree
            .root()
            .child_elements()
            .find(|c| c.local_name() == "style")
            .map(|c| (c.id(), c.namespace().to_owned()))
            .unwrap();
        tree.edit()
            .set_attr(style.0, &style.1, "customStyle", "1")
            .unwrap();
    });
    let e = scoped(&mut doc, ops, "p4").err().unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(e.message().contains("beyond the styles"), "{e}");
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
}
