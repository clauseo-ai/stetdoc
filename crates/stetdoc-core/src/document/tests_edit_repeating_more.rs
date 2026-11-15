//! More of the `row` operation on repeating-section items (issue 27, review): merged cells that cross items follow the
//! row the new row lands next to, a new item's id is unique in the whole package, and a new item does not say that it
//! shows its placeholder.

use super::tests_edit::*;
use super::tests_edit_repeating::{W15, document_xml, insert, item, open, outcome, table};
use super::*;
use crate::api::edit::Edit;
use crate::xml::NodeRef;
use stetdoc_testkit::fixtures::{parts, zip::Entry, zip::Zip};

const START: &str = r#"<w:vMerge w:val="restart"/>"#;
const CONT: &str = "<w:vMerge/>";

fn cell(text: &str, merge: &str) -> String {
    let p = if text.is_empty() {
        "<w:p/>".to_owned()
    } else {
        format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
    };
    format!(r#"<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/>{merge}</w:tcPr>{p}</w:tc>"#)
}

fn row2(a: &str, a_merge: &str, b: &str) -> String {
    format!("<w:tr>{}{}</w:tr>", cell(a, a_merge), cell(b, ""))
}

/// A two-column table that is one repeating section, then a plain row.
fn package2(items: &str) -> Vec<u8> {
    let body = format!(
        concat!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="4800" w:type="dxa"/></w:tblPr>"#,
            r#"<w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid>"#,
            r#"<w:sdt><w:sdtPr><w:id w:val="10"/><w15:repeatingSection/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
            r#"{}</w:tbl><w:p/>"#
        ),
        items,
        row2("Plain.", "", "plain")
    );
    let xml = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:w15="{W15}""#))
        .body(&body)
        .bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

/// M1 of the review: item 1 is A (plain) and B (a merge's start in the first column), item 2 is C, which continues it.
fn m1() -> Vec<u8> {
    package2(&format!(
        "{}{}",
        item(
            11,
            &format!("{}{}", row2("A", "", "a"), row2("B", START, "b"))
        ),
        item(12, &row2("", CONT, "c"))
    ))
}

/// M2 of the review: item 1 is A (a merge's start) and B (its continuation), item 2 is C (plain).
fn m2() -> Vec<u8> {
    package2(&format!(
        "{}{}",
        item(
            11,
            &format!("{}{}", row2("A", START, "a"), row2("", CONT, "b"))
        ),
        item(12, &row2("C", "", "c"))
    ))
}

/// Each row of the table, its cells as `text` with `^` for a merge's start and `|` for a continuation.
fn merges(doc: &Document) -> Vec<String> {
    fn rows<'a>(n: NodeRef<'a>, out: &mut Vec<NodeRef<'a>>) {
        for c in n.child_elements() {
            match c.local_name() {
                "tr" => out.push(c),
                "tc" => {}
                _ => rows(c, out),
            }
        }
    }
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let tree = view.tree(id).unwrap();
    let mut found = Vec::new();
    rows(tree.root(), &mut found);
    found
        .into_iter()
        .map(|tr| {
            tr.child_elements()
                .filter(|c| c.local_name() == "tc")
                .map(|tc| {
                    let merge = tc
                        .child_elements()
                        .find(|e| e.local_name() == "tcPr")
                        .and_then(|pr| pr.child_elements().find(|e| e.local_name() == "vMerge"))
                        .map(|m| match m.attrs().find(|a| a.local_name() == "val") {
                            Some(v) if v.value() == "restart" => "^",
                            _ => "|",
                        })
                        .unwrap_or("");
                    format!("{merge}{}", tc.text_content())
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}

/// Runs `op` untracked and tracked then accepted (with `outcome`'s checks), and returns the merges of both, which must agree.
fn merged_outcome(source: &[u8], op: &str) -> Vec<String> {
    outcome(source, op);
    let mut untracked = open(source);
    let b = batch(&untracked, op);
    untracked.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    let mut tracked = open(source);
    run(&mut tracked, op);
    run(&mut tracked, r#"{"op":"accept","all":true}"#);
    assert_eq!(
        merges(&untracked),
        merges(&tracked),
        "untracked and accepted"
    );
    merges(&tracked)
}

fn insert_cells(row: &str, place: &str, cells: &str) -> String {
    format!(r#"{{"op":"row","ref":"{row}","action":"insert","place":"{place}","cells":{cells}}}"#)
}

#[test]
fn m1_an_insert_after_the_plain_row_of_an_item_continues_the_merge_that_crosses_into_the_next_item()
{
    assert_eq!(
        merged_outcome(&m1(), &insert_cells("r1", "after", r#"["","n"]"#)),
        ["A a", "^B b", "| n", "| c", "Plain. plain"]
    );
    // The new row's first cell continues the merge, so text for it is refused, both ways.
    for untracked in [false, true] {
        let mut doc = open(&m1());
        let b = batch(&doc, &insert_cells("r1", "after", r#"["N","n"]"#));
        let edit = Edit::ops(b, stamp());
        let edit = if untracked { edit.untracked() } else { edit };
        let e = doc.edit(edit).err().unwrap();
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{}", e.message());
    }
}

#[test]
fn m2_an_insert_before_a_continuation_row_of_an_item_is_plain_above_the_item() {
    assert_eq!(
        merged_outcome(&m2(), &insert_cells("r2", "before", r#"["","n"]"#)),
        [" n", "^A a", "| b", "C c", "Plain. plain"]
    );
    assert_eq!(
        merged_outcome(&m2(), &insert_cells("r1", "after", r#"["N","n"]"#)),
        ["^A a", "| b", "N n", "C c", "Plain. plain"]
    );
}

/// A package whose header holds content controls with ids 13 and 21, and whose body is the issue's repeating section.
fn with_header() -> Vec<u8> {
    let body = format!(
        concat!(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4800"/></w:tblGrid>"#,
            r#"<w:sdt><w:sdtPr><w:id w:val="10"/><w15:repeatingSection/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt>"#,
            r#"</w:tbl><w:p/>"#
        ),
        item(
            11,
            "<w:tr><w:tc><w:p><w:r><w:t>One.</w:t></w:r></w:p></w:tc></w:tr>"
        ),
        item(
            12,
            "<w:tr><w:tc><w:p><w:r><w:t>Two.</w:t></w:r></w:p></w:tc></w:tr>"
        ),
    );
    let ns_w = parts::NS_W;
    let ns_r = parts::NS_R;
    let document = format!(
        r#"{}<w:document xmlns:w="{ns_w}" xmlns:r="{ns_r}" xmlns:w15="{W15}"><w:body>{body}<w:sectPr><w:headerReference w:type="default" r:id="rH"/></w:sectPr></w:body></w:document>"#,
        parts::DECL
    );
    let header = format!(
        r#"{}<w:hdr xmlns:w="{ns_w}"><w:sdt><w:sdtPr><w:id w:val="13"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Head.</w:t></w:r></w:p></w:sdtContent></w:sdt><w:sdt><w:sdtPr><w:id w:val="21"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>H2.</w:t></w:r></w:p></w:sdtContent></w:sdt></w:hdr>"#,
        parts::DECL
    );
    let types = parts::content_types().replace(
        "</Types>",
        r#"<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/></Types>"#,
    );
    let rels = format!(
        r#"{}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rH" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/header" Target="header1.xml"/></Relationships>"#,
        parts::DECL
    );
    let mut entries = parts::entries_with(document.as_bytes(), &parts::root_rels(), &types);
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new("word/header1.xml", header));
    Zip::new(entries).build()
}

#[test]
fn a_new_item_id_is_above_every_control_id_in_the_package_headers_included() {
    let source = with_header();
    let (_, done) = outcome(&source, &insert("r1", "after"));
    // The header's 21 is the highest id in the package, so the new item is 22, not 13 or 21.
    assert_eq!(
        done,
        "{10 section {11 item [One.]} {22 item [New row.]} {12 item [Two.]}}"
    );
}

#[test]
fn a_new_item_does_not_say_it_shows_its_placeholder() {
    let source = package2(&format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="11"/><w:showingPlcHdr/><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
        row2("Click here.", "", "b")
    ));
    for untracked in [false, true] {
        let mut doc = open(&source);
        let b = batch(&doc, &insert_cells("r1", "after", r#"["New.","n"]"#));
        let edit = Edit::ops(b, stamp());
        doc.edit(if untracked { edit.untracked() } else { edit })
            .unwrap();
        let xml = String::from_utf8(document_xml(&doc)).unwrap();
        assert_eq!(xml.matches("<w:showingPlcHdr/>").count(), 1, "{xml}");
        assert!(table(&doc).contains("{12 item ["), "{}", table(&doc));
        assert!(doc.check().ok, "{:?}", doc.check());
    }
}
