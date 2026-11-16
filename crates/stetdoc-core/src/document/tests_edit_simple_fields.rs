//! A simple field (`w:fldSimple`) takes the edits a complex field takes (issue #24): an insertion at either edge goes outside
//! it, a range, paragraph or row deletion that holds the whole field removes it, and a range that starts before it and ends
//! where its result ends replaces it (issue #23). A tracked deletion writes the field as a complex field whose runs are all
//! deleted, as Word does, since a `w:fldSimple` cannot stand in a `w:del`: accepting it removes the field, and rejecting it gives
//! the source back with that field as a complex field, with the same code and result, as Word's Reject All does. Changing
//! the cached result alone stays `readonly_region`. Each edit is checked tracked and accepted, tracked and rejected, and
//! untracked.

use super::tests_edit::*;
use super::tests_edit_field_ranges::{
    complex, edit, every_way, every_way_rejecting_to, main_xml, open, refused_both_ways, shape, wr,
};
use super::*;

fn simple(result: &str) -> String {
    format!(
        r#"<w:fldSimple w:instr=" PAGE ">{}</w:fldSimple>"#,
        wr(result)
    )
}

/// `See page {=PAGE|1} for details.` (`p1`), `Ends with {=PAGE|1}` (`p2`), then a table whose first row holds
/// `Page {=PAGE|1}` (`p3`) and whose second holds `Other row` (`p4`), then `Tail.` (`p5`).
fn fixture() -> String {
    fixture_with(None)
}

/// `fixture`, with its simple field number `unfolded` (1, 2 or 3) written as a complex field, as a tracked deletion of it
/// leaves it once rejected.
fn fixture_with(unfolded: Option<usize>) -> String {
    let field = |n: usize| {
        if unfolded == Some(n) {
            complex()
        } else {
            simple("1")
        }
    };
    let row = |inner: &str| format!("<w:tr><w:tc><w:tcPr/><w:p>{inner}</w:p></w:tc></w:tr>");
    format!(
        r#"<w:p>{}{}{}</w:p><w:p>{}{}</w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>{}{}</w:tbl><w:p>{}</w:p>"#,
        wr("See page "),
        field(1),
        wr(" for details."),
        wr("Ends with "),
        field(2),
        row(&format!("{}{}", wr("Page "), field(3))),
        row(&wr("Other row")),
        wr("Tail.")
    )
}

const REST: &str = "Ends with {=PAGE|1} / Page {=PAGE|1} / Other row / Tail.";

/// `every_way_rejecting_to`, with the rejected main part `rejected`'s byte for byte, which holds when every edge of the edit
/// is at the edge of a run, so that nothing is cut.
fn every_way_exactly(body: &str, rejected: &str, ops: &str, want: &str) {
    every_way_rejecting_to(body, rejected, ops, want);
    let mut doc = open(body);
    edit(&mut doc, ops, true).unwrap();
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(
        main_xml(&doc),
        main_xml(&open(rejected)),
        "{ops}, then reject"
    );
}

#[test]
fn an_insertion_at_either_edge_of_a_simple_field_goes_outside_it() {
    let body = fixture();
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":9,"end":9,"with":"X"}"#,
        &format!("See page X{{=PAGE|1}} for details. / {REST}"),
    );
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":10,"end":10,"with":"X"}"#,
        &format!("See page {{=PAGE|1}}X for details. / {REST}"),
    );
    // At the end of a paragraph that ends with the field.
    every_way(
        &body,
        r#"{"op":"replace","ref":"p2","start":11,"end":11,"with":"."}"#,
        "See page {=PAGE|1} for details. / Ends with {=PAGE|1}. / Page {=PAGE|1} / Other row / Tail.",
    );
    // A replacement that starts right after the field.
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":10,"end":23,"with":"; elsewhere"}"#,
        &format!("See page {{=PAGE|1}}; elsewhere / {REST}"),
    );
}

#[test]
fn a_range_that_holds_a_simple_field_deletes_it_and_accepting_removes_the_element() {
    let body = fixture();
    let first = fixture_with(Some(1));
    let doc = every_way_rejecting_to(
        &body,
        &first,
        r#"{"op":"delete","ref":"p1","start":8,"end":11}"#,
        &format!("See pagefor details. / {REST}"),
    );
    // Tracked, the field is a complex field whose runs are all deleted, as Word writes it, and it is gone in `Final`.
    let xml = main_xml(&doc);
    assert!(
        xml.contains(concat!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:delInstrText xml:space="preserve"> PAGE </w:delInstrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:delText xml:space="preserve">1</w:delText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        )),
        "{xml}"
    );
    // The deletions around the field and the field's are one, as Word writes them.
    assert_eq!(xml.matches("<w:del ").count(), 1, "{xml}");
    assert_eq!(xml.matches("<w:fldSimple").count(), 2, "{xml}");
    assert_eq!(final_text(&doc, "p1"), "See pagefor details.");
    let line = final_view_line(&doc, "p1");
    assert!(!line.contains("{field"), "{line}");
    // Exactly the field: its result, deleted, deletes the field, as for a complex field.
    every_way_exactly(
        &body,
        &first,
        r#"{"op":"delete","ref":"p1","start":9,"end":10}"#,
        &format!("See page  for details. / {REST}"),
    );
    // From before the field to the end of its result, and from the field to past it (issue #23).
    every_way_rejecting_to(
        &body,
        &first,
        r#"{"op":"replace","ref":"p1","start":8,"end":10,"with":"X"}"#,
        &format!("See pageX for details. / {REST}"),
    );
    every_way_rejecting_to(
        &body,
        &first,
        r#"{"op":"replace","ref":"p1","start":9,"end":11,"with":"X"}"#,
        &format!("See page Xfor details. / {REST}"),
    );
    every_way_rejecting_to(
        &body,
        &fixture_with(Some(2)),
        r#"{"op":"replace","ref":"p2","with":"new text"}"#,
        "See page {=PAGE|1} for details. / new text / Page {=PAGE|1} / Other row / Tail.",
    );
}

#[test]
fn a_paragraph_or_a_row_that_holds_a_simple_field_can_be_deleted() {
    let body = fixture();
    every_way_exactly(
        &body,
        &fixture_with(Some(1)),
        r#"{"op":"delete","ref":"p1"}"#,
        REST,
    );
    every_way_exactly(
        &body,
        &fixture_with(Some(3)),
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
        "See page {=PAGE|1} for details. / Ends with {=PAGE|1} / Other row / Tail.",
    );
    // A paragraph before a table goes with its field, as the body does not need it (issue 25).
    every_way_exactly(
        &body,
        &fixture_with(Some(2)),
        r#"{"op":"delete","ref":"p2"}"#,
        "See page {=PAGE|1} for details. / Page {=PAGE|1} / Other row / Tail.",
    );
}

#[test]
fn a_simple_field_is_formatted_with_its_paragraph() {
    let body = fixture();
    every_way_exactly(
        &body,
        &body,
        r#"{"op":"format","ref":"p1","bold":true}"#,
        &shape(&open(&body)),
    );
    let mut doc = open(&body);
    edit(&mut doc, r#"{"op":"format","ref":"p1","bold":true}"#, false).unwrap();
    let xml = main_xml(&doc);
    assert!(
        xml.contains(r#"<w:fldSimple w:instr=" PAGE "><w:r><w:rPr><w:b/></w:rPr>"#),
        "{xml}"
    );
}

#[test]
fn changing_a_simple_fields_result_alone_stays_refused() {
    let body = fixture();
    refused_both_ways(
        &body,
        r#"{"op":"replace","ref":"p1","start":9,"end":10,"with":"2"}"#,
    );
    // Inside a longer result.
    let long = format!("<w:p>{}{}</w:p>", wr("See page "), simple("12"));
    refused_both_ways(
        &long,
        r#"{"op":"replace","ref":"p1","start":10,"end":10,"with":"X"}"#,
    );
    refused_both_ways(&long, r#"{"op":"delete","ref":"p1","start":9,"end":10}"#);
}

#[test]
fn a_simple_field_with_field_data_goes_whole() {
    let body = format!(
        r#"<w:p>{}<w:fldSimple w:instr=" PAGE "><w:fldData xml:space="preserve">AAAA</w:fldData>{}</w:fldSimple>{}</w:p>"#,
        wr("See page "),
        wr("1"),
        wr(" for details.")
    );
    // The field data goes into the begin of the complex field that a tracked deletion writes.
    let rejected = format!(
        r#"<w:p>{}<w:r><w:fldChar w:fldCharType="begin"><w:fldData xml:space="preserve">AAAA</w:fldData></w:fldChar></w:r>{}</w:p>"#,
        wr("See page "),
        complex()
            .trim_start_matches(r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#)
            .to_owned()
            + &wr(" for details.")
    );
    every_way_exactly(
        &body,
        &rejected,
        r#"{"op":"delete","ref":"p1","start":9,"end":10}"#,
        "See page  for details.",
    );
    let mut doc = open(&body);
    edit(
        &mut doc,
        r#"{"op":"delete","ref":"p1","start":9,"end":10}"#,
        false,
    )
    .unwrap();
    assert!(!main_xml(&doc).contains("fldData"), "{}", main_xml(&doc));
}

fn link(inner: &str) -> String {
    format!(r#"<w:hyperlink w:anchor="target">{inner}</w:hyperlink>"#)
}

/// Runs `ops` untracked, and tracked then accepted: neither leaves an empty hyperlink or simple field.
fn leaves_no_empty_wrapper(body: &str, ops: &str) {
    let mut doc = open(body);
    edit(&mut doc, ops, false).unwrap();
    let untracked = main_xml(&doc);
    let mut doc = open(body);
    edit(&mut doc, ops, true).unwrap();
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    let accepted = main_xml(&doc);
    for xml in [untracked, accepted] {
        assert!(
            !xml.contains("<w:hyperlink") && !xml.contains("<w:fldSimple"),
            "{ops}: {xml}"
        );
    }
}

/// The line of `token` in the `Final` view, which shows a field that is still there as `{field ...}`.
fn final_view_line(doc: &Document, token: &str) -> String {
    use crate::api::view::{Mode as M, ViewSpec};
    let spec = ViewSpec {
        mode: M::Final,
        ..ViewSpec::default()
    };
    doc.view(&spec)
        .unwrap()
        .text
        .lines()
        .find(|l| l.starts_with(&format!("{token} ")) || *l == token)
        .unwrap_or_default()
        .to_string()
}

#[test]
fn deleting_a_link_that_ends_with_a_simple_field_removes_both() {
    // The link is empty only once the field inside it went.
    let body = format!(
        "<w:p>{}{}</w:p>",
        link(&format!("{}{}", wr("See "), simple("1"))),
        wr(" tail")
    );
    let ops = r#"{"op":"delete","ref":"p1","start":0,"end":5}"#;
    let rejected = format!(
        "<w:p>{}{}</w:p>",
        link(&format!("{}{}", wr("See "), complex())),
        wr(" tail")
    );
    every_way_rejecting_to(&body, &rejected, ops, " tail");
    leaves_no_empty_wrapper(&body, ops);
}

#[test]
fn a_simple_field_whose_result_is_a_link_is_gone_in_final_once_deleted() {
    // The tracked deletion goes inside the link, inside the field: the field is gone in `Final`, as accepting removes it.
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        wr("See "),
        simple_with(&link(&wr("1"))),
        wr(" tail")
    );
    let ops = r#"{"op":"delete","ref":"p1","start":4,"end":5}"#;
    let [open_runs, close_run] = [
        concat!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#
        ),
        r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
    ];
    let rejected = format!(
        "<w:p>{}{open_runs}{}{close_run}{}</w:p>",
        wr("See "),
        link(&wr("1")),
        wr(" tail")
    );
    let doc = every_way_rejecting_to(&body, &rejected, ops, "See  tail");
    let line = final_view_line(&doc, "p1");
    assert!(!line.contains("{field"), "{line}");
    leaves_no_empty_wrapper(&body, ops);
}

fn simple_with(inner: &str) -> String {
    format!(r#"<w:fldSimple w:instr=" PAGE ">{inner}</w:fldSimple>"#)
}

#[test]
fn a_simple_field_with_no_content_goes_untracked_and_is_refused_tracked() {
    // `Alpha {=PAGE|}beta.`, then `Tail.` and `Last {=PAGE|}one.`, which ends the body and is kept when deleted.
    let empty = r#"<w:fldSimple w:instr=" PAGE "/>"#;
    let body = format!(
        "<w:p>{}{empty}{}</w:p><w:p>{}</w:p><w:p>{}{empty}{}</w:p>",
        wr("Alpha "),
        wr("beta."),
        wr("Tail."),
        wr("Last "),
        wr("one.")
    );
    for (ops, want) in [
        (
            r#"{"op":"delete","ref":"p1","start":4,"end":8}"#,
            "Alphta. / Tail. / Last {=PAGE|}one.",
        ),
        (
            r#"{"op":"replace","ref":"p1","start":4,"end":8,"with":"X"}"#,
            "AlphXta. / Tail. / Last {=PAGE|}one.",
        ),
        (r#"{"op":"delete","ref":"p1"}"#, "Tail. / Last {=PAGE|}one."),
        (
            r#"{"op":"delete","ref":"p3"}"#,
            "Alpha {=PAGE|}beta. / Tail.",
        ),
    ] {
        let mut doc = open(&body);
        edit(&mut doc, ops, false).unwrap_or_else(|e| panic!("{ops}: {e}"));
        assert_eq!(shape(&doc), want, "{ops}: {}", main_xml(&doc));
        assert!(doc.check().ok, "{ops}: {:?}", doc.check());
        let mut doc = open(&body);
        let e = edit(&mut doc, ops, true)
            .err()
            .unwrap_or_else(|| panic!("{ops} tracked was applied"));
        assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{ops}: {e}");
        assert!(e.to_string().contains("untracked"), "{ops}: {e}");
    }
}

/// The runs of a complex PAGE field with the result `1`, each with the properties `rpr`, as a tracked deletion of a simple
/// field writes them, deleted (`deleted`) or after rejecting.
fn complex_with(rpr: &str, deleted: bool) -> String {
    let (code, text) = if deleted {
        ("delInstrText", "delText")
    } else {
        ("instrText", "t")
    };
    format!(
        concat!(
            r#"<w:r>{0}<w:fldChar w:fldCharType="begin"/></w:r><w:r>{0}<w:{1} xml:space="preserve"> PAGE </w:{1}></w:r>"#,
            r#"<w:r>{0}<w:fldChar w:fldCharType="separate"/></w:r><w:r>{0}<w:{2} xml:space="preserve">1</w:{2}></w:r>"#,
            r#"<w:r>{0}<w:fldChar w:fldCharType="end"/></w:r>"#
        ),
        rpr, code, text
    )
}

#[test]
fn a_tracked_paragraph_deletion_writes_the_field_as_word_does() {
    // Word's own tracked deletion of the paragraph (issue #24, Word for Mac 16.114): the mark deleted, and one `w:del` holding
    // the text and the field as a complex field.
    let mut doc = open(&fixture());
    edit(&mut doc, r#"{"op":"delete","ref":"p1"}"#, true).unwrap();
    let xml = main_xml(&doc);
    let p1 = xml
        .split("<w:body>")
        .nth(1)
        .and_then(|b| b.split("</w:p>").next())
        .unwrap_or_default();
    let del = p1.split("<w:del ").nth(2).unwrap_or_default();
    let want = format!(
        r#"<w:r><w:delText xml:space="preserve">See page </w:delText></w:r>{}<w:r><w:delText xml:space="preserve"> for details.</w:delText></w:r></w:del>"#,
        complex_with("", true)
    );
    assert!(p1.contains("<w:pPr><w:rPr><w:del "), "{p1}");
    assert_eq!(p1.matches("<w:del ").count(), 2, "{p1}");
    assert!(del.ends_with(&want), "{p1}");
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn the_complex_field_keeps_the_simple_fields_formatting_and_attributes() {
    let body = format!(
        r#"<w:p>{}<w:fldSimple w:instr=" PAGE " w:fldLock="1" w:dirty="true"><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">1</w:t></w:r></w:fldSimple>{}</w:p>"#,
        wr("See page "),
        wr(" for details.")
    );
    let rejected = format!(
        "<w:p>{}{}{}</w:p>",
        wr("See page "),
        complex_with("<w:rPr><w:b/></w:rPr>", false).replacen(
            r#"w:fldCharType="begin"/>"#,
            r#"w:fldCharType="begin" w:fldLock="1" w:dirty="true"/>"#,
            1
        ),
        wr(" for details.")
    );
    every_way_exactly(
        &body,
        &rejected,
        r#"{"op":"delete","ref":"p1","start":9,"end":10}"#,
        "See page  for details.",
    );
    // A pending format of the result: the field's runs take its formatting from before that change, so that rejecting both
    // leaves no trace of it.
    let changed = body.replace(
        "<w:rPr><w:b/></w:rPr>",
        r#"<w:rPr><w:i/><w:rPrChange w:id="90" w:author="Other" w:date="2026-01-01T00:00:00Z"><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr>"#,
    );
    let mut doc = open(&changed);
    edit(
        &mut doc,
        r#"{"op":"delete","ref":"p1","start":9,"end":10}"#,
        true,
    )
    .unwrap();
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(main_xml(&doc), main_xml(&open(&rejected)));
}

#[test]
fn a_simple_field_inside_another_is_written_as_a_complex_field_with_it() {
    // The outer field is deleted whole only once the inner one is a complex field.
    let body = format!(
        r#"<w:p>{}<w:fldSimple w:instr=" IF 1 = 1 ">{}</w:fldSimple></w:p><w:p>{}</w:p>"#,
        wr("A "),
        simple("1"),
        wr("Tail.")
    );
    let ops = r#"{"op":"delete","ref":"p1"}"#;
    let mut doc = open(&body);
    edit(&mut doc, ops, true).unwrap();
    assert!(
        !main_xml(&doc).contains("<w:fldSimple"),
        "{}",
        main_xml(&doc)
    );
    assert!(doc.check().ok, "{:?}", doc.check());
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(shape(&doc), "A {IF 1 = 1|{PAGE|1}} / Tail.");
    assert!(doc.check().ok, "{:?}", doc.check());
    let mut doc = open(&body);
    edit(&mut doc, ops, true).unwrap();
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    assert_eq!(shape(&doc), "Tail.");
}
