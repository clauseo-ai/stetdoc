//! Run properties out of the order Word writes (`w:color` before `w:b`), through every kind of edit: a run the edit creates or
//! cuts off gets its properties in schema order, the runs the document had keep their bytes, and `check` counts a copy of the
//! defect as a problem the edit added. The Open XML SDK counts each such `w:rPr` as a schema error, so a copy would be a new
//! error to it.

use super::tests_edit::*;
use super::tests_tamper::arm;
use super::*;
use crate::api::check::FindingCode;
use crate::api::edit::Edit;
use crate::api::options::{CompareOptions, Markup};
use crate::refs::Claim;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

/// Out of order: `w:b` comes before `w:color` in `CT_RPr`.
const BAD: &str = r#"<w:rPr><w:color w:val="1F3864"/><w:b/></w:rPr>"#;
/// The same properties in schema order.
const GOOD: &str = r#"<w:rPr><w:b/><w:color w:val="1F3864"/></w:rPr>"#;

fn body(p1: &str) -> String {
    let p2 = untouched();
    let p3 = format!(
        concat!(
            r#"<w:p><w:r>{b}<w:t xml:space="preserve">Kept text and </w:t></w:r>"#,
            r#"<w:ins w:id="7" w:author="Opposing" w:date="2026-01-01T00:00:00Z"><w:r>{b}<w:t>inserted words</w:t></w:r></w:ins>"#,
            r#"<w:r>{b}<w:t xml:space="preserve"> more kept.</w:t></w:r></w:p>"#
        ),
        b = BAD
    );
    format!(r#"<w:p><w:r>{BAD}<w:t>{p1}</w:t></w:r></w:p>{p2}{p3}"#)
}

/// p2, which no edit below touches.
fn untouched() -> String {
    format!(r#"<w:p><w:r>{BAD}<w:t>This paragraph is never edited.</w:t></w:r></w:p>"#)
}

fn package(p1: &str) -> Vec<u8> {
    let xml = parts::Doc::new().body(&body(p1)).bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

fn open(bytes: Vec<u8>) -> Document {
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

fn fixture() -> Document {
    open(package("The supplier shall deliver the goods on time."))
}

fn main_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

/// How many `schema_non_canonical_order` findings `check` makes in the whole document.
fn out_of_order(doc: &Document) -> u32 {
    doc.check()
        .by_code
        .iter()
        .find(|(c, _)| *c == FindingCode::SchemaNonCanonicalOrder)
        .map_or(0, |(_, n)| u32::try_from(*n).unwrap())
}

#[test]
fn no_edit_copies_run_properties_out_of_order_and_untouched_runs_keep_their_bytes() {
    let input = fixture();
    let (bad_in, findings_in) = (main_xml(&input).matches(BAD).count(), out_of_order(&input));
    assert_eq!((bad_in, findings_in), (5, 5));
    let cases: [(&str, Markup); 12] = [
        (
            r#"{"op":"replace","ref":"p1","find":"goods","with":"products"}"#,
            Markup::Revisions,
        ),
        (
            r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["A new paragraph."]}"#,
            Markup::Revisions,
        ),
        (
            r#"{"op":"delete","ref":"p1","find":"the goods"}"#,
            Markup::Revisions,
        ),
        (
            r#"{"op":"format","ref":"p1","find":"goods","italic":true}"#,
            Markup::Revisions,
        ),
        (r#"{"op":"split","ref":"p1","at":12}"#, Markup::Revisions),
        (
            r#"{"op":"comment","ref":"p1","find":"goods","text":"Check this."}"#,
            Markup::Revisions,
        ),
        (
            r#"{"op":"replace","ref":"p3","find":"inserted","with":"added"}"#,
            Markup::Revisions,
        ),
        (
            r#"{"op":"replace","ref":"p3","find":"text","with":"words"}"#,
            Markup::Revisions,
        ),
        (r#"{"op":"accept","all":true}"#, Markup::Revisions),
        (r#"{"op":"reject","all":true}"#, Markup::Revisions),
        (
            r#"{"op":"replace","ref":"p1","find":"goods","with":"products"}"#,
            Markup::Manual,
        ),
        (
            r#"{"op":"delete","ref":"p1","find":"on time"}"#,
            Markup::ManualColor,
        ),
    ];
    for (op, markup) in cases {
        let mut doc = fixture();
        doc.edit(Edit::ops(batch(&doc, op), stamp()).markup(markup))
            .unwrap_or_else(|e| panic!("{op}: {e}"));
        let after = main_xml(&doc);
        assert!(after.contains(&untouched()), "{op}: p2 changed: {after}");
        assert!(
            after.matches(BAD).count() <= bad_in,
            "{op}: a copy of the out-of-order properties: {after}"
        );
        assert!(out_of_order(&doc) <= findings_in, "{op}: {:?}", doc.check());
    }
}

#[test]
fn a_replace_cuts_the_run_and_only_the_pieces_it_made_are_reordered() {
    let mut doc = fixture();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"goods","with":"products"}"#,
    );
    let after = main_xml(&doc);
    // The run the document had keeps its bytes; the deleted piece, the inserted text and the rest are new runs, in order.
    for piece in [
        format!(
            r#"<w:r>{BAD}<w:t xml:space="preserve">The supplier shall deliver the </w:t></w:r>"#
        ),
        format!(r#"<w:r>{GOOD}<w:delText>goods</w:delText></w:r>"#),
        format!(r#"<w:r>{GOOD}<w:t>products</w:t></w:r>"#),
        format!(r#"<w:r>{GOOD}<w:t xml:space="preserve"> on time.</w:t></w:r>"#),
    ] {
        assert!(after.contains(&piece), "{piece} not in {after}");
    }
    assert_eq!(after.matches(BAD).count(), 5, "{after}");
}

#[test]
fn compare_writes_the_runs_it_makes_in_order() {
    let original = package("The supplier shall deliver the goods on time.");
    let revised = package("The supplier must deliver the products on time.");
    let base = open(revised.clone());
    let options = CompareOptions {
        limits: crate::api::limits::Limits::default(),
        stamp: stamp(),
        format: crate::api::view::FormatId::stet(),
    };
    let redline = compare(original, revised, &options).unwrap().document;
    assert!(out_of_order(&redline) <= out_of_order(&base));
    assert!(main_xml(&redline).contains(GOOD));
}

#[test]
fn a_copy_of_out_of_order_properties_is_refused_as_a_problem_the_edit_added() {
    // A compiler that copied the first run as it was would add a second out-of-order `w:rPr` to p1, which already had one: the key is
    // the input's, and the count is what is new.
    let mut doc = fixture();
    let before = doc.save().unwrap().bytes;
    let key = doc.state.resolve(Claim::Implied, "p1").unwrap();
    arm(move |tx| {
        let tree = tx.part_mut(key.part()).unwrap();
        let run = tree
            .node_ref(key.node())
            .child_elements()
            .find(|c| c.local_name() == "r")
            .unwrap()
            .id();
        let copy = tree.edit().clone_subtree(run).unwrap();
        tree.edit().append_child(key.node(), copy).unwrap();
    });
    let op = r#"{"op":"replace","ref":"p1","find":"goods","with":"products"}"#;
    let e = doc.edit(Edit::ops(batch(&doc, op), stamp())).err().unwrap();
    assert_eq!(e.code(), ErrorCode::ValidationFailed, "{e}");
    assert!(e.message().contains("not in the order Word writes"), "{e}");
    assert_eq!(doc.save().unwrap().bytes, before);
    // Without the copy the same edit is accepted, and the receipt says the one out of order was already there.
    let receipt = run(&mut doc, op);
    assert!(
        receipt.validation.carried_total >= 1,
        "{:?}",
        receipt.validation
    );
    assert_eq!(
        receipt.validation.carried[0].code,
        FindingCode::SchemaNonCanonicalOrder
    );
}
