//! Another author's pending change through `Document::edit`, tracked and untracked (design `revisions.md` decision 4 and 5.5).
//! Tracked edits record what they do to it, or refuse; untracked edits never rewrite or hide it, and edit the author's own
//! pending changes in place.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

const DATE: &str = r#"w:date="2026-01-01T00:00:00Z""#;

/// p1 holds Opposing's insertion between plain text, p2 Opposing's paragraph formatting change, p3 a run with Opposing's
/// formatting change, p4 Clauseo's own pending insertion, and p5 is plain.
fn with_changes() -> Document {
    let body = format!(
        concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">Plain start </w:t></w:r><w:ins w:id="101" w:author="Opposing" {d}><w:r><w:t>inserted words</w:t></w:r></w:ins><w:r><w:t xml:space="preserve"> plain end.</w:t></w:r></w:p>"#,
            r#"<w:p><w:pPr><w:jc w:val="center"/><w:pPrChange w:id="102" w:author="Opposing" {d}><w:pPr/></w:pPrChange></w:pPr><w:r><w:t>Centred by Opposing.</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:rPr><w:b/><w:rPrChange w:id="103" w:author="Opposing" {d}><w:rPr/></w:rPrChange></w:rPr><w:t>Bold by Opposing.</w:t></w:r></w:p>"#,
            r#"<w:p><w:ins w:id="104" w:author="Clauseo" {d}><w:r><w:t>Clauseo wrote this.</w:t></w:r></w:ins></w:p>"#,
            r#"<w:p><w:r><w:t>Plain paragraph.</w:t></w:r></w:p>"#,
        ),
        d = DATE
    );
    let xml = parts::Doc::new().body(&body).bytes();
    let doc = Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap();
    assert!(doc.check().ok, "{:?}", doc.check());
    doc
}

fn main_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

/// Runs `ops` on a fresh document, tracked or not.
fn edit(ops: &str, tracked: bool) -> (Document, Result<Receipt, StetError>) {
    let mut doc = with_changes();
    let b = batch(&doc, ops);
    let e = if tracked {
        Edit::ops(b, stamp())
    } else {
        Edit::ops(b, stamp()).untracked()
    };
    let result = doc.edit(e);
    (doc, result)
}

/// The edit is refused as `readonly_region` naming Opposing, and the document is as it was.
fn refused(ops: &str, tracked: bool) -> StetError {
    let before = with_changes().save().unwrap().bytes;
    let (doc, result) = edit(ops, tracked);
    let e = result
        .err()
        .unwrap_or_else(|| panic!("{ops} (tracked: {tracked}) was applied"));
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{ops}: {e}");
    assert!(e.message().contains("Opposing"), "{ops}: {e}");
    assert_eq!(doc.save().unwrap().bytes, before, "{ops}");
    e
}

fn applied(ops: &str, tracked: bool) -> Document {
    let (doc, result) = edit(ops, tracked);
    result.unwrap_or_else(|e| panic!("{ops} (tracked: {tracked}): {e}"));
    assert!(doc.check().ok, "{ops}: {:?}", doc.check());
    doc
}

#[test]
fn text_inside_another_authors_insertion_is_tracked_or_refused_and_never_rewritten_untracked() {
    let ops = r#"{"op":"replace","ref":"p1","find":"inserted","with":"added"}"#;
    // Tracked, the deletion nests inside Opposing's insertion and the new text is Clauseo's.
    let doc = applied(ops, true);
    assert_eq!(final_text(&doc, "p1"), "Plain start added words plain end.");
    let xml = main_xml(&doc);
    assert!(
        xml.contains(r#"w:author="Opposing""#) && xml.contains(r#"w:author="Clauseo""#),
        "{xml}"
    );
    // Untracked, the new text would be Opposing's, so it is refused and says what to do.
    let e = refused(ops, false);
    assert!(e.message().contains("pending insertion by Opposing"), "{e}");
    assert!(
        e.message().contains("accept or reject that change first"),
        "{e}"
    );
    // A deletion inside it, and a split inside it, are refused untracked too.
    refused(r#"{"op":"delete","ref":"p1","find":"inserted"}"#, false);
    refused(r#"{"op":"split","ref":"p1","at":15}"#, false);
    // At its edge, untracked text goes beside it and is nobody's insertion.
    let doc = applied(
        r#"{"op":"replace","ref":"p1","start":26,"end":26,"with":"!"}"#,
        false,
    );
    assert_eq!(
        final_text(&doc, "p1"),
        "Plain start inserted words! plain end."
    );
    assert!(
        main_xml(&doc).contains("<w:t>inserted words</w:t></w:r></w:ins>"),
        "{}",
        main_xml(&doc)
    );
}

#[test]
fn formatting_under_another_authors_change_is_refused_tracked_and_untracked() {
    for tracked in [true, false] {
        let e = refused(r#"{"op":"paragraph","ref":"p2","align":"left"}"#, tracked);
        assert!(e.message().contains("Opposing"), "{e}");
        refused(r#"{"op":"format","ref":"p3","italic":true}"#, tracked);
        refused(
            r#"{"op":"set","ref":"p3","target":"run","props":[{"name":"vanish"}]}"#,
            tracked,
        );
    }
}

#[test]
fn hiding_another_authors_inserted_run_is_tracked_or_refused() {
    let ops = r#"{"op":"set","ref":"p1","find":"inserted words","props":[{"name":"vanish"}]}"#;
    // Tracked, the run gets Clauseo's formatting change inside Opposing's insertion, and a reject takes it back.
    let doc = applied(ops, true);
    assert!(main_xml(&doc).contains("w:rPrChange"), "{}", main_xml(&doc));
    // Untracked, it would vanish with no record.
    let e = refused(ops, false);
    assert!(e.message().contains("pending insertion by Opposing"), "{e}");
}

#[test]
fn removing_a_paragraph_with_another_authors_change_is_refused_untracked() {
    let doc = applied(r#"{"op":"delete","ref":"p1"}"#, true);
    assert!(main_xml(&doc).contains("<w:del "), "{}", main_xml(&doc));
    for ops in [
        r#"{"op":"delete","ref":"p1"}"#,
        r#"{"op":"delete","ref":"p3"}"#,
        r#"{"op":"merge","ref":"p2","second":"p3"}"#,
    ] {
        refused(ops, false);
    }
}

#[test]
fn the_authors_own_pending_changes_are_edited_in_place_untracked() {
    let doc = applied(
        r#"{"op":"replace","ref":"p4","find":"wrote","with":"typed"}"#,
        false,
    );
    assert_eq!(final_text(&doc, "p4"), "Clauseo typed this.");
    let xml = main_xml(&doc);
    assert!(!xml.contains("<w:del "), "nothing was recorded: {xml}");
    // And plain text is edited untracked as before.
    let doc = applied(
        r#"{"op":"paragraph","ref":"p5","align":"right"},{"op":"merge","ref":"p4","second":"p5"}"#,
        false,
    );
    assert!(final_text(&doc, "p4").starts_with("Clauseo wrote this."));
}
