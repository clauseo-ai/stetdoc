//! Resolving a tracked cell that is its row's only cell, through `Document::edit` (decision 0024). Rejecting an inserted
//! cell or accepting a deleted one removes the cell, and a row has at least one cell, so the row goes with it, and the
//! table with its last row. Accepting the insertion or rejecting the deletion keeps the row. Each is checked by all, by
//! author and by id.

use super::tests_edit::*;
use super::*;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

const DAN: &str = r#"w:author="Dan" w:date="2024-01-01T00:00:00Z""#;

fn cell(text: &str, mark: &str) -> String {
    format!(
        r#"<w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/>{mark}</w:tcPr><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:tc>"#
    )
}

fn ins_mark() -> String {
    format!(r#"<w:cellIns w:id="301" {DAN}/>"#)
}

fn del_mark() -> String {
    format!(r#"<w:cellDel w:id="302" {DAN}/>"#)
}

fn table(rows: &[String]) -> String {
    let rows: String = rows.iter().map(|r| format!("<w:tr>{r}</w:tr>")).collect();
    format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid>{rows}</w:tbl>"#
    )
}

fn package(table: &str) -> Vec<u8> {
    let body = format!(
        "<w:p><w:r><w:t>Before</w:t></w:r></w:p>{table}<w:p><w:r><w:t>After</w:t></w:r></w:p>"
    );
    let xml = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

/// A table of three rows whose middle row is one marked cell.
fn middle(mark: &str) -> Vec<u8> {
    package(&table(&[
        cell("Kept row", ""),
        cell("Only cell", mark),
        cell("Last row", ""),
    ]))
}

/// A table of one row that is one marked cell.
fn lone(mark: &str) -> Vec<u8> {
    package(&table(&[cell("Only cell", mark)]))
}

/// The body's text, a `|` for each row and a `#` for each table.
fn shape(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let xml = String::from_utf8(view.tree(id).unwrap().serialize()).unwrap();
    let mut out = String::new();
    let mut rest = xml.as_str();
    while let Some(i) = rest.find('<') {
        rest = &rest[i + 1..];
        let end = rest.find('>').unwrap();
        let tag = &rest[..end];
        let name = tag.split([' ', '/']).next().unwrap();
        match name {
            "w:tbl" => out.push('#'),
            "w:tr" => out.push('|'),
            "w:t" if !tag.ends_with('/') => {
                let text = &rest[end + 1..];
                out.push_str(&text[..text.find('<').unwrap()]);
                out.push(' ');
            }
            _ => {}
        }
        rest = &rest[end + 1..];
    }
    out.trim_end().to_owned()
}

/// Resolves the source by each selector and returns the one shape they agree on, each result checked.
fn resolved(source: &[u8], action: &str, id: u32) -> String {
    let mut shapes = Vec::new();
    for select in [
        r#""all":true"#.to_owned(),
        r#""author":"Dan""#.to_owned(),
        format!(r#""ids":[{id}]"#),
    ] {
        let mut doc = Document::open(source.to_vec(), &OpenOptions::default()).unwrap();
        run(&mut doc, &format!(r#"{{"op":"{action}",{select}}}"#));
        let check = doc.check();
        assert!(check.ok, "{select}: {check:?}");
        assert!(
            doc.revisions(&RevisionFilter::default())
                .unwrap()
                .is_empty(),
            "{select}: a revision is left"
        );
        shapes.push(shape(&doc));
    }
    assert!(shapes.windows(2).all(|w| w[0] == w[1]), "{shapes:?}");
    shapes.remove(0)
}

#[test]
fn rejecting_an_inserted_cell_that_is_its_rows_only_cell_removes_the_row() {
    assert_eq!(
        resolved(&middle(&ins_mark()), "reject", 301),
        "Before #|Kept row |Last row After"
    );
}

#[test]
fn rejecting_an_inserted_cell_that_is_its_tables_only_cell_removes_the_table() {
    assert_eq!(resolved(&lone(&ins_mark()), "reject", 301), "Before After");
}

#[test]
fn accepting_a_deleted_cell_that_is_its_rows_only_cell_removes_the_row() {
    assert_eq!(
        resolved(&middle(&del_mark()), "accept", 302),
        "Before #|Kept row |Last row After"
    );
}

#[test]
fn accepting_a_deleted_cell_that_is_its_tables_only_cell_removes_the_table() {
    assert_eq!(resolved(&lone(&del_mark()), "accept", 302), "Before After");
}

#[test]
fn accepting_the_insertion_or_rejecting_the_deletion_of_a_rows_only_cell_keeps_the_row() {
    let kept = "Before #|Kept row |Only cell |Last row After";
    assert_eq!(resolved(&middle(&ins_mark()), "accept", 301), kept);
    assert_eq!(resolved(&middle(&del_mark()), "reject", 302), kept);
    assert_eq!(
        resolved(&lone(&ins_mark()), "accept", 301),
        "Before #|Only cell After"
    );
    assert_eq!(
        resolved(&lone(&del_mark()), "reject", 302),
        "Before #|Only cell After"
    );
}

#[test]
fn a_row_goes_only_when_its_last_cell_goes() {
    // Two inserted cells in one row: rejecting both removes the row; a row that keeps a cell stays.
    let two = format!(
        "{}{}",
        cell("One", &ins_mark()),
        cell("Two", &format!(r#"<w:cellIns w:id="303" {DAN}/>"#))
    );
    let source = package(&table(&[cell("Kept row", ""), two]));
    let mut doc = Document::open(source.clone(), &OpenOptions::default()).unwrap();
    run(&mut doc, r#"{"op":"reject","all":true}"#);
    assert!(doc.check().ok, "{:?}", doc.check());
    assert_eq!(shape(&doc), "Before #|Kept row After");
    let mut doc = Document::open(source, &OpenOptions::default()).unwrap();
    run(&mut doc, r#"{"op":"reject","ids":[301]}"#);
    assert!(doc.check().ok, "{:?}", doc.check());
    assert_eq!(shape(&doc), "Before #|Kept row |Two After");
}

/// Clauseo's two documents (issue report, text made up): a one-row table whose row is one inserted cell, and a control
/// whose row has a plain cell beside the inserted one. Both end with an empty paragraph.
fn clauseo(cells: &str, grid: &str) -> Vec<u8> {
    let body = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">Sample clause one.</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid>{grid}</w:tblGrid><w:tr>{cells}</w:tr></w:tbl><w:p><w:r><w:t xml:space="preserve"></w:t></w:r></w:p>"#
    );
    let xml = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

#[test]
fn clauseos_one_cell_row_rejected_by_id_removes_the_table_and_the_two_cell_control_keeps_its_row() {
    let reviewer = r#"<w:cellIns w:id="101" w:author="Reviewer" w:date="2026-11-01T00:00:00Z"/>"#;
    let col = r#"<w:gridCol w:w="4000"/>"#;
    let one = clauseo(&cell("Inserted cell A.", reviewer), col);
    let two = clauseo(
        &format!(
            "{}{}",
            cell("Cell A.", ""),
            cell("Inserted cell B.", reviewer)
        ),
        &format!("{col}{col}"),
    );
    for (source, after) in [
        (one, "Sample clause one."),
        (two, "Sample clause one. #|Cell A."),
    ] {
        let mut doc = Document::open(source, &OpenOptions::default()).unwrap();
        run(&mut doc, r#"{"op":"reject","ids":[101]}"#);
        assert!(doc.check().ok, "{:?}", doc.check());
        assert_eq!(shape(&doc), after);
    }
}
