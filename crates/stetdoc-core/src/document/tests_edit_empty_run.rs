//! An empty run is no content (issue #19). A run whose only text is an empty `w:t` does not keep a deleted paragraph alive:
//! deleting the paragraph and accepting removes it and leaves the next one as it was, as it does when the empty run is not
//! there. The tracked deletion holds the empty run with the text around it, and rejecting it gives back the original.

use super::tests_edit::*;
use super::*;
use stetdoc_testkit::fixtures::parts;
use stetdoc_testkit::fixtures::zip::Zip;

const NEXT: &str = r#"<w:p w14:paraId="10000002"><w:r><w:t xml:space="preserve">Following paragraph.</w:t></w:r></w:p>"#;

/// `p1` reads `Alpha beta gamma.` with `empty` between its two runs, and `p2` is [`NEXT`].
fn document(empty: &str) -> Document {
    with_first(&format!(
        r#"<w:r><w:t xml:space="preserve">Alpha </w:t></w:r>{empty}<w:r><w:t xml:space="preserve">beta gamma.</w:t></w:r>"#
    ))
}

/// `p1` holds `inner`, and `p2` is [`NEXT`].
fn with_first(inner: &str) -> Document {
    let body = format!(r#"<w:p w14:paraId="10000001">{inner}</w:p>{NEXT}"#);
    let xml = parts::Doc::new()
        .root_attrs(r#" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml""#)
        .body(&body)
        .bytes();
    let bytes = Zip::new(parts::entries_doc(&xml)).build();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

fn main_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

#[test]
fn deleting_a_paragraph_with_an_empty_run_removes_it_and_leaves_the_next_one() {
    for empty in [
        "",
        "<w:r><w:t/></w:r>",
        "<w:r><w:t></w:t></w:r>",
        r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve"></w:t></w:r>"#,
    ] {
        let mut doc = document(empty);
        let original = main_xml(&doc);
        assert_eq!(final_text(&doc, "p1"), "Alpha beta gamma.", "{empty}");
        run(&mut doc, r#"{"op":"delete","ref":"p1"}"#);
        // One deletion of the text, with the empty run inside it, and the deleted mark.
        let tracked = main_xml(&doc);
        assert_eq!(tracked.matches("<w:del ").count(), 2, "{empty}: {tracked}");
        assert!(!tracked.contains("<w:t/>"), "{empty}: {tracked}");
        assert!(!tracked.contains("<w:t></w:t>"), "{empty}: {tracked}");
        // Accepted: p1 is deleted and p2 is as it was, with its paraId.
        let receipt = run(&mut doc, r#"{"op":"accept","all":true}"#);
        let retired: Vec<(&str, &str)> = receipt
            .retired
            .iter()
            .map(|r| (r.r#ref.as_str(), r.reason.as_str()))
            .collect();
        assert_eq!(retired, [("p1", "deleted")], "{empty}: {}", receipt.text);
        let accepted = main_xml(&doc);
        assert!(accepted.contains(NEXT), "{empty}: {accepted}");
        assert!(!accepted.contains("10000001"), "{empty}: {accepted}");
        assert_eq!(final_text(&doc, "p2"), "Following paragraph.", "{empty}");
        // Rejected instead: the original, but that an empty text element is written in its short form.
        let mut doc = document(empty);
        run(&mut doc, r#"{"op":"delete","ref":"p1"}"#);
        run(&mut doc, r#"{"op":"reject","all":true}"#);
        let short = |xml: String| xml.replace("></w:t>", "/>");
        assert_eq!(short(main_xml(&doc)), short(original), "{empty}");
    }
}

#[test]
fn an_empty_run_at_either_end_of_a_deleted_paragraph_does_not_keep_it_either() {
    // The deletion does not reach a run beyond the text, so here only the join's rule decides.
    let text = r#"<w:r><w:t xml:space="preserve">Alpha beta gamma.</w:t></w:r>"#;
    for inner in [
        format!("<w:r><w:t/></w:r>{text}"),
        format!("{text}<w:r><w:rPr><w:i/></w:rPr><w:t></w:t></w:r>"),
    ] {
        let mut doc = with_first(&inner);
        run(&mut doc, r#"{"op":"delete","ref":"p1"}"#);
        let receipt = run(&mut doc, r#"{"op":"accept","all":true}"#);
        assert_eq!(receipt.retired.len(), 1, "{inner}: {}", receipt.text);
        assert_eq!(receipt.retired[0].r#ref, "p1", "{inner}");
        assert_eq!(receipt.retired[0].reason, "deleted", "{inner}");
        let accepted = main_xml(&doc);
        assert!(accepted.contains(NEXT), "{inner}: {accepted}");
        assert!(!accepted.contains("10000001"), "{inner}: {accepted}");
    }
}

#[test]
fn a_stray_empty_deleted_text_run_stays_out_of_the_deletion_and_comes_back_from_a_rejection() {
    // An empty `w:delText` outside any `w:del` is not ours to wrap: wrapped, a rejection would give it back as a `w:t`.
    let mut doc = with_first(
        r#"<w:r><w:t xml:space="preserve">Alpha </w:t></w:r><w:r><w:delText/></w:r><w:r><w:t xml:space="preserve">beta.</w:t></w:r>"#,
    );
    let original = main_xml(&doc);
    run(&mut doc, r#"{"op":"delete","ref":"p1"}"#);
    assert!(
        main_xml(&doc).contains("</w:del><w:r><w:delText/></w:r><w:del "),
        "{}",
        main_xml(&doc)
    );
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(main_xml(&doc), original);
}
