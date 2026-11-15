//! A simple field (`w:fldSimple`) takes the edits a complex field takes (issue #24): an insertion at either edge goes outside
//! it, a range, paragraph or row deletion that holds the whole field removes it, and a range that starts before it and ends
//! where its result ends replaces it (issue #23). A tracked deletion wraps the field's runs in `w:del` inside the element, which
//! cannot stand in a `w:del`; accepting it removes the element, and rejecting it gives the source back byte for byte. Changing
//! the cached result alone stays `readonly_region`. Each edit is checked tracked and accepted, tracked and rejected, and
//! untracked.

use super::tests_edit::*;
use super::tests_edit_field_ranges::{
    edit, every_way, main_xml, open, refused_both_ways, shape, wr,
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
    let row = |inner: &str| format!("<w:tr><w:tc><w:tcPr/><w:p>{inner}</w:p></w:tc></w:tr>");
    format!(
        r#"<w:p>{}{}{}</w:p><w:p>{}{}</w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>{}{}</w:tbl><w:p>{}</w:p>"#,
        wr("See page "),
        simple("1"),
        wr(" for details."),
        wr("Ends with "),
        simple("1"),
        row(&format!("{}{}", wr("Page "), simple("1"))),
        row(&wr("Other row")),
        wr("Tail.")
    )
}

const REST: &str = "Ends with {=PAGE|1} / Page {=PAGE|1} / Other row / Tail.";

/// `every_way`, with the rejected main part the source's byte for byte, which holds when every edge of the edit is at the edge
/// of a run, so that nothing is cut.
fn every_way_exactly(body: &str, ops: &str, want: &str) {
    every_way(body, ops, want);
    let mut doc = open(body);
    edit(&mut doc, ops, true).unwrap();
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert_eq!(main_xml(&doc), main_xml(&open(body)), "{ops}, then reject");
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
    let doc = every_way(
        &body,
        r#"{"op":"delete","ref":"p1","start":8,"end":11}"#,
        &format!("See pagefor details. / {REST}"),
    );
    // Tracked, the field's run is deleted inside the element, and the field is gone in `Final`.
    let xml = main_xml(&doc);
    assert!(
        xml.contains(r#"<w:fldSimple w:instr=" PAGE "><w:del "#),
        "{xml}"
    );
    assert_eq!(final_text(&doc, "p1"), "See pagefor details.");
    let line = final_view_line(&doc, "p1");
    assert!(!line.contains("{field"), "{line}");
    // Exactly the field: its result, deleted, deletes the field, as for a complex field.
    every_way_exactly(
        &body,
        r#"{"op":"delete","ref":"p1","start":9,"end":10}"#,
        &format!("See page  for details. / {REST}"),
    );
    // From before the field to the end of its result, and from the field to past it (issue #23).
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":8,"end":10,"with":"X"}"#,
        &format!("See pageX for details. / {REST}"),
    );
    every_way(
        &body,
        r#"{"op":"replace","ref":"p1","start":9,"end":11,"with":"X"}"#,
        &format!("See page Xfor details. / {REST}"),
    );
    every_way(
        &body,
        r#"{"op":"replace","ref":"p2","with":"new text"}"#,
        "See page {=PAGE|1} for details. / new text / Page {=PAGE|1} / Other row / Tail.",
    );
}

#[test]
fn a_paragraph_or_a_row_that_holds_a_simple_field_can_be_deleted() {
    let body = fixture();
    every_way_exactly(&body, r#"{"op":"delete","ref":"p1"}"#, REST);
    every_way_exactly(
        &body,
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
        "See page {=PAGE|1} for details. / Ends with {=PAGE|1} / Other row / Tail.",
    );
    // A paragraph that ends its container is kept, empty, and the field leaves it.
    every_way_exactly(
        &body,
        r#"{"op":"delete","ref":"p2"}"#,
        "See page {=PAGE|1} for details. /  / Page {=PAGE|1} / Other row / Tail.",
    );
}

#[test]
fn a_simple_field_is_formatted_with_its_paragraph() {
    let body = fixture();
    every_way_exactly(
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
    every_way_exactly(
        &body,
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
    every_way(&body, ops, " tail");
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
    let doc = every_way(&body, ops, "See  tail");
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
            "Alpha {=PAGE|}beta. / Tail. / ",
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
