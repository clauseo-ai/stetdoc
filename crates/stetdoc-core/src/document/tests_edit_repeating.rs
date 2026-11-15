//! The `row` operation on the items of a repeating section, through `Document::edit` (issue 27): a new row next to an
//! item's row is the content of a new item beside that item, deleting an item's only row removes the item, and deleting
//! every item's row removes the section. Each batch runs untracked, and tracked then accepted, and the tracked one
//! rejected gives the source back byte for byte.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::xml::NodeRef;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

pub(super) const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

fn row(text: &str) -> String {
    format!(
        r#"<w:tr><w:tc><w:tcPr><w:tcW w:w="4800" w:type="dxa"/></w:tcPr><w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p></w:tc></w:tr>"#
    )
}

pub(super) fn item(id: u32, rows: &str) -> String {
    format!(
        r#"<w:sdt><w:sdtPr><w:id w:val="{id}"/><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent>{rows}</w:sdtContent></w:sdt>"#
    )
}

fn package(items: &str) -> Vec<u8> {
    let body = format!(
        concat!(
            r#"<w:p><w:r><w:t>Lead paragraph.</w:t></w:r></w:p>"#,
            r#"<w:tbl><w:tblPr><w:tblW w:w="4800" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4800"/></w:tblGrid>"#,
            r#"<w:sdt><w:sdtPr><w:id w:val="10"/><w15:repeatingSection/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
            r#"{}</w:tbl><w:p><w:r><w:t>Last paragraph.</w:t></w:r></w:p>"#
        ),
        items,
        row("Plain row.")
    );
    let xml = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:w15="{W15}""#))
        .body(&body)
        .bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

/// `a23-repeating.docx` of the issue: two items of one row each, then a plain row.
fn repeating() -> Vec<u8> {
    package(&format!(
        "{}{}",
        item(11, &row("Item one.")),
        item(12, &row("Item two."))
    ))
}

pub(super) fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &OpenOptions::default()).unwrap()
}

pub(super) fn document_xml(doc: &Document) -> Vec<u8> {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    view.tree(id).unwrap().serialize()
}

/// The table as the issue writes it: a control is `{id section ...}` or `{id item ...}`, a row is `[text]`, and a row
/// with a tracked change says so.
pub(super) fn table(doc: &Document) -> String {
    fn walk(n: NodeRef<'_>, out: &mut Vec<String>) {
        for c in n.child_elements() {
            match c.local_name() {
                "sdt" => {
                    let pr = c.child_elements().find(|e| e.local_name() == "sdtPr");
                    let id = pr
                        .and_then(|p| p.child_elements().find(|e| e.local_name() == "id"))
                        .and_then(|e| e.attrs().find(|a| a.local_name() == "val"))
                        .map(|a| a.value().into_owned())
                        .unwrap_or_default();
                    let kind = pr
                        .and_then(|p| {
                            p.child_elements().find_map(|e| match e.local_name() {
                                "repeatingSection" => Some("section"),
                                "repeatingSectionItem" => Some("item"),
                                _ => None,
                            })
                        })
                        .unwrap_or("control");
                    let mut inner = Vec::new();
                    walk(c, &mut inner);
                    let mut s = format!("{{{id} {kind}");
                    for i in inner {
                        s.push(' ');
                        s.push_str(&i);
                    }
                    s.push('}');
                    out.push(s);
                }
                "sdtContent" => walk(c, out),
                "tr" => {
                    let pr = c.child_elements().find(|e| e.local_name() == "trPr");
                    let mark = pr
                        .and_then(|p| {
                            p.child_elements().find_map(|e| match e.local_name() {
                                "ins" => Some("+"),
                                "del" => Some("-"),
                                _ => None,
                            })
                        })
                        .unwrap_or("");
                    out.push(format!("[{mark}{}]", c.text_content()));
                }
                _ => {}
            }
        }
    }
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let tree = view.tree(id).unwrap();
    let mut stack = vec![tree.root()];
    while let Some(n) = stack.pop() {
        if n.local_name() == "tbl" {
            let mut out = Vec::new();
            walk(n, &mut out);
            return out.join(" ");
        }
        stack.extend(n.child_elements());
    }
    String::new()
}

/// Runs `op` untracked, and tracked then accepted, checks that the two agree, that rejecting the tracked edit gives the
/// source back byte for byte and that `check` stays clean, and returns the tracked table before review and the result.
pub(super) fn outcome(source: &[u8], op: &str) -> (String, String) {
    let mut untracked = open(source);
    let b = batch(&untracked, op);
    untracked
        .edit(Edit::ops(b, stamp()).untracked())
        .expect("untracked edit");
    let mut tracked = open(source);
    run(&mut tracked, op);
    let pending = table(&tracked);
    assert!(tracked.check().ok, "{:?}", tracked.check());
    let mut rejected = open(source);
    run(&mut rejected, op);
    run(&mut rejected, r#"{"op":"reject","all":true}"#);
    assert_eq!(
        String::from_utf8(document_xml(&rejected)).unwrap(),
        String::from_utf8(document_xml(&open(source))).unwrap(),
        "reject-all gives the source back"
    );
    run(&mut tracked, r#"{"op":"accept","all":true}"#);
    assert_eq!(table(&untracked), table(&tracked), "untracked and accepted");
    for doc in [&untracked, &tracked, &rejected] {
        assert!(doc.check().ok, "{:?}", doc.check());
    }
    (pending, table(&tracked))
}

pub(super) fn insert(row: &str, place: &str) -> String {
    format!(
        r#"{{"op":"row","ref":"{row}","action":"insert","place":"{place}","cells":["New row."]}}"#
    )
}

fn delete(row: &str) -> String {
    format!(r#"{{"op":"row","ref":"{row}","action":"delete"}}"#)
}

#[test]
fn the_source_reads_as_the_issue_shows_it() {
    assert_eq!(
        table(&open(&repeating())),
        "{10 section {11 item [Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
}

#[test]
fn an_insert_after_an_item_row_is_a_new_item_after_that_item() {
    let (pending, done) = outcome(&repeating(), &insert("r1", "after"));
    assert_eq!(
        pending,
        "{10 section {11 item [Item one.]} {13 item [+New row.]} {12 item [Item two.]}} [Plain row.]"
    );
    assert_eq!(
        done,
        "{10 section {11 item [Item one.]} {13 item [New row.]} {12 item [Item two.]}} [Plain row.]"
    );
}

#[test]
fn an_insert_after_the_last_item_row_is_a_new_last_item() {
    let (_, done) = outcome(&repeating(), &insert("r2", "after"));
    assert_eq!(
        done,
        "{10 section {11 item [Item one.]} {12 item [Item two.]} {13 item [New row.]}} [Plain row.]"
    );
}

#[test]
fn an_insert_before_the_first_item_row_is_a_new_first_item() {
    let (_, done) = outcome(&repeating(), &insert("r1", "before"));
    assert_eq!(
        done,
        "{10 section {13 item [New row.]} {11 item [Item one.]} {12 item [Item two.]}} [Plain row.]"
    );
}

#[test]
fn an_insert_after_a_plain_row_makes_no_item() {
    let (_, done) = outcome(&repeating(), &insert("r3", "after"));
    assert_eq!(
        done,
        "{10 section {11 item [Item one.]} {12 item [Item two.]}} [Plain row.] [New row.]"
    );
}

#[test]
fn deleting_an_item_only_row_removes_the_item() {
    let (pending, done) = outcome(&repeating(), &delete("r2"));
    assert_eq!(
        pending,
        "{10 section {11 item [Item one.]} {12 item [-Item two.]}} [Plain row.]"
    );
    assert_eq!(done, "{10 section {11 item [Item one.]}} [Plain row.]");
}

#[test]
fn deleting_every_item_row_removes_the_section() {
    let (_, done) = outcome(&repeating(), &format!("{},{}", delete("r1"), delete("r2")));
    assert_eq!(done, "[Plain row.]");
}

#[test]
fn deleting_one_row_of_an_item_with_two_keeps_the_item() {
    let source = package(&format!(
        "{}{}",
        item(11, &format!("{}{}", row("Name."), row("Address."))),
        item(12, &row("Item two."))
    ));
    let (_, done) = outcome(&source, &delete("r2"));
    assert_eq!(
        done,
        "{10 section {11 item [Name.]} {12 item [Item two.]}} [Plain row.]"
    );
}

#[test]
fn a_new_item_copies_the_item_properties_with_a_new_id_and_no_binding() {
    let props = concat!(
        r#"<w:alias w:val="Party"/><w:tag w:val="party"/><w:id w:val="11"/>"#,
        r#"<w:dataBinding w:xpath="/a[1]" w:storeItemID="{00000000-0000-0000-0000-000000000000}"/>"#,
        r#"<w15:repeatingSectionItem/>"#
    );
    let source = package(&format!(
        r#"<w:sdt><w:sdtPr>{props}</w:sdtPr><w:sdtEndPr/><w:sdtContent>{}</w:sdtContent></w:sdt>"#,
        row("Item one.")
    ));
    let mut doc = open(&source);
    run(&mut doc, &insert("r1", "after"));
    let xml = String::from_utf8(document_xml(&doc)).unwrap();
    assert_eq!(
        xml.matches(r#"<w:alias w:val="Party"/>"#).count(),
        2,
        "{xml}"
    );
    assert_eq!(xml.matches(r#"<w:tag w:val="party"/>"#).count(), 2);
    assert_eq!(xml.matches("<w:sdtEndPr/>").count(), 2);
    assert_eq!(xml.matches("<w:dataBinding").count(), 1);
    assert!(xml.contains(r#"<w:id w:val="12"/>"#), "{xml}");
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn an_inserted_item_whose_row_is_deleted_in_the_same_batch_goes() {
    let source = repeating();
    let mut doc = open(&source);
    let receipt = run(&mut doc, &insert("r1", "after"));
    let new_row = receipt
        .added
        .iter()
        .find(|r| r.starts_with('r'))
        .unwrap()
        .clone();
    run(&mut doc, &delete(&new_row));
    assert_eq!(
        String::from_utf8(document_xml(&doc)).unwrap(),
        String::from_utf8(document_xml(&open(&source))).unwrap()
    );
}
