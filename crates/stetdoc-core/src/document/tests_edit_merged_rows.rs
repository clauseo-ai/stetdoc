//! The `row` operation on tables with merged cells, through `Document::edit` (issue 21): a new row continues a vertical
//! merge only inside it, a cell that continues a merge takes no text, and deleting a merge's start row moves the start to
//! the row below. Each batch runs untracked, and tracked then accepted, and the tracked one rejected gives the source back.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::stamp::Stamp;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

fn cell(text: &str, merge: &str) -> String {
    let p = if text.is_empty() {
        "<w:p/>".to_owned()
    } else {
        format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
    };
    format!(r#"<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/>{merge}</w:tcPr>{p}</w:tc>"#)
}

const START: &str = r#"<w:vMerge w:val="restart"/>"#;
const CONT: &str = "<w:vMerge/>";
const GRID2: &str = r#"<w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid>"#;

fn package(table: &str) -> Vec<u8> {
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr>{table}</w:tbl><w:p/>"#
    );
    let xml = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&xml)).build()
}

/// `a29-vmerge.docx`: the first column is merged over rows 1 to 3, row 4 is plain.
fn vmerge() -> Vec<u8> {
    package(&format!(
        "{GRID2}<w:tr>{}{}</w:tr><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr><w:tr>{}{}</w:tr>",
        cell("Seller", START),
        cell("Obligation one.", ""),
        cell("", CONT),
        cell("Obligation two.", ""),
        cell("", CONT),
        cell("Obligation three.", ""),
        cell("Buyer", ""),
        cell("Obligation four.", ""),
    ))
}

/// `a29-hmerge.docx`: row 1 is one legacy horizontally merged cell, row 2 is plain.
fn hmerge() -> Vec<u8> {
    package(&format!(
        "{GRID2}<w:tr>{}{}</w:tr><w:tr>{}{}</w:tr>",
        cell("Schedule heading.", r#"<w:hMerge w:val="restart"/>"#),
        cell("", "<w:hMerge/>"),
        cell("Left.", ""),
        cell("Right.", ""),
    ))
}

/// The table of the main story, one row per line: each cell is its merge flag and its text.
fn table(doc: &Document) -> Vec<String> {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    // A row's properties start with `<w:tr` too, so they are renamed before the rows are split.
    let xml = String::from_utf8(view.tree(id).unwrap().serialize())
        .unwrap()
        .replace("<w:trPr", "<w:rowPr");
    let mut lines = Vec::new();
    for row in xml
        .split("<w:tr")
        .skip(1)
        .filter(|r| r.starts_with('>') || r.starts_with(' '))
    {
        let row = row.split("</w:tr>").next().unwrap();
        let cells: Vec<String> = row
            .split("<w:tc>")
            .skip(1)
            .map(|c| {
                let flag = if c.contains(r#"<w:vMerge w:val="restart"/>"#) {
                    "v:start "
                } else if c.contains("<w:vMerge/>") {
                    "v:cont "
                } else if c.contains(r#"<w:hMerge w:val="restart"/>"#) {
                    "h:start "
                } else if c.contains("<w:hMerge/>") {
                    "h:cont "
                } else {
                    ""
                };
                let text: String = c
                    .split("<w:t")
                    .skip(1)
                    .filter(|t| t.starts_with('>') || t.starts_with(' '))
                    .filter_map(|t| t.split_once('>').map(|(_, rest)| rest))
                    .filter_map(|t| t.split("</w:t>").next())
                    .collect();
                format!("{flag}'{text}'")
            })
            .collect();
        lines.push(cells.join(" | "));
    }
    lines
}

fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &OpenOptions::default()).unwrap()
}

/// Runs `op` untracked, and tracked then accepted, and checks that the two agree, that rejecting the tracked edit gives
/// the source back, and returns the table. A refusal must be the same both ways, and is returned.
fn outcome(source: &[u8], op: &str) -> Result<Vec<String>, StetError> {
    let mut untracked = open(source);
    let b = batch(&untracked, op);
    let plain = untracked.edit(Edit::ops(b, stamp()).untracked());
    let mut tracked = open(source);
    let b = batch(&tracked, op);
    let edited = tracked.edit(Edit::ops(b, stamp()));
    match (plain, edited) {
        (Err(a), Err(b)) => {
            assert_eq!((a.code(), a.message()), (b.code(), b.message()));
            Err(a)
        }
        (Ok(_), Ok(_)) => {
            let mut rejected = open(source);
            run(&mut rejected, op);
            run(&mut rejected, r#"{"op":"reject","all":true}"#);
            assert_eq!(table(&rejected), table(&open(source)), "reject-all");
            run(&mut tracked, r#"{"op":"accept","all":true}"#);
            assert_eq!(table(&untracked), table(&tracked), "untracked and accepted");
            for doc in [&untracked, &tracked] {
                assert!(doc.check().ok, "{:?}", doc.check());
            }
            Ok(table(&tracked))
        }
        (a, b) => panic!("untracked {:?}, tracked {:?}", a.err(), b.err()),
    }
}

fn insert(row: &str, place: &str, cells: &str) -> String {
    format!(r#"{{"op":"row","ref":"{row}","action":"insert","place":"{place}","cells":{cells}}}"#)
}

fn refused(source: &[u8], op: &str, column: usize) {
    let e = outcome(source, op).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(
        e.message()
            .contains(&format!("column {column} of the new row continues"))
            && e.message()
                .contains(&format!("Pass \"\" for column {column}.")),
        "{e}"
    );
}

const TAIL: [&str; 3] = [
    "v:cont '' | 'Obligation two.'",
    "v:cont '' | 'Obligation three.'",
    "'Buyer' | 'Obligation four.'",
];

#[test]
fn a_new_row_inside_a_vertical_merge_refuses_text_for_the_continuation_and_takes_an_empty_one() {
    let both = r#"["New left.","New right."]"#;
    let empty = r#"["","New right."]"#;
    refused(&vmerge(), &insert("r1", "after", both), 1);
    refused(&vmerge(), &insert("r2", "before", both), 1);
    for op in [insert("r1", "after", empty), insert("r2", "before", empty)] {
        let mut want = vec![
            "v:start 'Seller' | 'Obligation one.'".to_owned(),
            "v:cont '' | 'New right.'".to_owned(),
        ];
        want.extend(TAIL.iter().map(|s| (*s).to_owned()));
        assert_eq!(outcome(&vmerge(), &op).unwrap(), want, "{op}");
    }
}

#[test]
fn a_new_row_below_a_vertical_merge_or_above_it_is_plain() {
    let both = r#"["New left.","New right."]"#;
    let rows = outcome(&vmerge(), &insert("r3", "after", both)).unwrap();
    assert_eq!(
        rows.get(3).unwrap(),
        "'New left.' | 'New right.'",
        "{rows:?}"
    );
    assert_eq!(rows.get(2).unwrap(), "v:cont '' | 'Obligation three.'");
    let rows = outcome(&vmerge(), &insert("r1", "before", both)).unwrap();
    assert_eq!(
        rows.first().unwrap(),
        "'New left.' | 'New right.'",
        "{rows:?}"
    );
    assert_eq!(rows.get(1).unwrap(), "v:start 'Seller' | 'Obligation one.'");
    let rows = outcome(&vmerge(), &insert("r4", "after", both)).unwrap();
    assert_eq!(
        rows.get(4).unwrap(),
        "'New left.' | 'New right.'",
        "{rows:?}"
    );
}

#[test]
fn deleting_the_row_of_a_vertical_merge_start_makes_the_next_cell_the_start() {
    let delete = r#"{"op":"row","ref":"r1","action":"delete"}"#;
    assert_eq!(
        outcome(&vmerge(), delete).unwrap(),
        [
            "v:start '' | 'Obligation two.'",
            "v:cont '' | 'Obligation three.'",
            "'Buyer' | 'Obligation four.'",
        ]
    );
    // Tracked, the merge stays as it was until the deletion is accepted.
    let mut doc = open(&vmerge());
    run(&mut doc, delete);
    assert_eq!(table(&doc).get(1).unwrap(), "v:cont '' | 'Obligation two.'");
    // Deleting a continuation row shrinks the merge.
    let rows = outcome(&vmerge(), r#"{"op":"row","ref":"r2","action":"delete"}"#).unwrap();
    assert_eq!(
        rows,
        [
            "v:start 'Seller' | 'Obligation one.'",
            "v:cont '' | 'Obligation three.'",
            "'Buyer' | 'Obligation four.'",
        ]
    );
}

#[test]
fn a_new_row_under_a_legacy_horizontal_merge_refuses_text_for_its_continuation() {
    refused(
        &hmerge(),
        &insert("r1", "after", r#"["New left.","New right."]"#),
        2,
    );
    assert_eq!(
        outcome(&hmerge(), &insert("r1", "after", r#"["New heading.",""]"#)).unwrap(),
        [
            "h:start 'Schedule heading.' | h:cont ''",
            "h:start 'New heading.' | h:cont ''",
            "'Left.' | 'Right.'",
        ]
    );
}

#[test]
fn a_merge_is_found_by_grid_column_and_not_by_cell_index() {
    // Row 1 has three cells and its merge starts in the third; row 2 spans the first two columns with one cell, so its
    // continuation is its second cell, in the same grid column.
    let grid = r#"<w:tblGrid><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/><w:gridCol w:w="2400"/></w:tblGrid>"#;
    let wide = r#"<w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr><w:p><w:r><w:t>Wide</w:t></w:r></w:p></w:tc>"#;
    let source = package(&format!(
        "{grid}<w:tr>{}{}{}</w:tr><w:tr>{wide}{}</w:tr><w:tr>{}{}{}</w:tr>",
        cell("A", ""),
        cell("B", ""),
        cell("Merged", START),
        cell("", CONT),
        cell("C", ""),
        cell("D", ""),
        cell("E", ""),
    ));
    let rows = outcome(&source, &insert("r1", "after", r#"["x","y",""]"#)).unwrap();
    assert_eq!(rows.get(1).unwrap(), "'x' | 'y' | v:cont ''", "{rows:?}");
    let rows = outcome(&source, r#"{"op":"row","ref":"r1","action":"delete"}"#).unwrap();
    assert_eq!(rows.first().unwrap(), "'Wide' | v:start ''", "{rows:?}");
    let rows = outcome(&source, &insert("r2", "after", r#"["x","z"]"#)).unwrap();
    assert_eq!(rows.get(2).unwrap(), "'x' | 'z'", "{rows:?}");
}

#[test]
fn rejecting_an_inserted_row_that_became_a_merge_start_moves_the_start_below() {
    // Alice inserts a row inside the merge, Bob deletes the start row, and Bob's change is accepted, so Alice's row holds
    // the start. Rejecting everything then removes her row, and the next continuation becomes the start.
    let mut doc = open(&vmerge());
    let as_who = |doc: &mut Document, who: &str, op: &str| {
        let b = batch(doc, op);
        doc.edit(Edit::ops(b, Stamp::new(who, "2026-10-06T10:00:00")))
            .unwrap();
    };
    as_who(&mut doc, "Alice", &insert("r1", "after", r#"["","n"]"#));
    as_who(
        &mut doc,
        "Bob",
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
    );
    as_who(&mut doc, "Bob", r#"{"op":"accept","author":"Bob"}"#);
    {
        let view = doc.state.parts();
        let id = view.id("word/document.xml").unwrap();
        eprintln!(
            "XML {}",
            String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
        );
    }
    assert_eq!(
        table(&doc).first().unwrap(),
        "v:start '' | 'n'",
        "{:?}",
        table(&doc)
    );
    as_who(&mut doc, "Bob", r#"{"op":"reject","all":true}"#);
    assert_eq!(
        table(&doc),
        ["v:start '' | 'Obligation two.'"]
            .into_iter()
            .chain(TAIL.iter().skip(1).copied())
            .collect::<Vec<_>>()
    );
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn rejecting_every_inserted_row_gives_the_source_back() {
    // A row that stetdoc inserts never holds a start from the source, so moving the start on a rejected insertion
    // never fires on a rejection that gives the source back: each insert, rejected, is the source.
    for op in [
        insert("r1", "before", r#"["a","b"]"#),
        insert("r1", "after", r#"["","b"]"#),
        insert("r2", "before", r#"["","b"]"#),
        insert("r2", "after", r#"["","b"]"#),
        insert("r3", "after", r#"["a","b"]"#),
        insert("r4", "after", r#"["a","b"]"#),
    ] {
        let mut doc = open(&vmerge());
        run(&mut doc, &op);
        run(&mut doc, r#"{"op":"reject","all":true}"#);
        assert_eq!(table(&doc), table(&open(&vmerge())), "{op}");
    }
}
