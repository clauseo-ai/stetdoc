//! Known answers for accept and reject on rows, tables and the paragraphs they leave: what a removed row keeps, a
//! cell that ends with a table, rows in content controls, and a story that must keep a paragraph.

use super::tests_resolve::{accept_all, del, ins, reject_all, run};
use super::tests_support::*;

fn cell_of(inner: &str) -> String {
    format!("<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{inner}</w:tc>")
}

fn table_of(rows: &str) -> String {
    format!("<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid>{rows}</w:tbl>")
}

#[test]
fn accepting_a_deleted_row_moves_a_comment_reference_run_into_the_paragraph_before_and_not_into_the_table()
 {
    // A run cannot stand in a w:tbl, a range marker can; what a removed row keeps goes to the end of the paragraph before
    // it, in order, so the range still ends before its reference.
    let deleted = format!(
        "<w:tr><w:trPr><w:del {}/></w:trPr>{}</w:tr>",
        eric(1),
        cell_of(&format!(
            "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr><w:commentRangeStart w:id=\"0\"/>{}<w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>",
            eric(2),
            del(3, "gone")
        ))
    );
    let kept = format!("<w:tr>{}</w:tr>", cell_of(&p(&r("kept"))));
    let body = format!(
        "{}{}{}",
        p(&r("before")),
        table_of(&format!("{deleted}{kept}")),
        p(&r("after"))
    );
    let a = run(&body, &[accept_all()]);
    assert!(
        a.xml.contains("before</w:t></w:r><w:commentRangeStart w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p><w:tbl>"),
        "{}",
        a.xml
    );
    assert_eq!(a.fin, ["before", "kept", "after"]);
    // The first row of a table has no paragraph before it in the table: the one before the table takes them, and with none
    // the one after.
    let first = format!(
        "{}{}",
        table_of(&format!("{deleted}{kept}")),
        p(&r("after"))
    );
    let b = run(&first, &[accept_all()]);
    assert!(
        b.xml.contains("<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p><w:commentRangeStart w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r><w:r><w:t xml:space=\"preserve\">kept</w:t>"),
        "{}",
        b.xml
    );
}

#[test]
fn accepting_a_row_in_a_nested_table_leaves_a_cell_that_ends_with_that_table_as_it_was() {
    // LibreOffice and Word write cells that end with a table; accepting a row inside it must not add a paragraph to the Final
    // view.
    let inner = table_of(&format!(
        "<w:tr>{}</w:tr><w:tr><w:trPr><w:ins {}/></w:trPr>{}</w:tr>",
        cell_of(&p(&r("a"))),
        eric(1),
        cell_of(&format!(
            "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p>",
            eric(2),
            ins(3, "new")
        ))
    ));
    let body = format!(
        "{}{}",
        table_of(&format!("<w:tr>{}</w:tr>", cell_of(&inner))),
        p(&r("after"))
    );
    let a = run(&body, &[accept_all()]);
    assert!(
        a.xml.contains("</w:tbl></w:tc></w:tr></w:tbl>"),
        "{}",
        a.xml
    );
    assert!(!a.xml.contains("<w:p/>"), "{}", a.xml);
    assert_eq!(a.fin, ["a", "new", "after"]);
}

#[test]
fn a_paragraph_after_a_nested_table_that_goes_leaves_the_cell_the_paragraph_it_must_end_with() {
    // Accepting the deletion of the paragraph after a nested table, or rejecting the insertion of one, gives the cell
    // the empty paragraph it must end with (check's W9). Even when the cell ended with the table before the
    // insertion, as LibreOffice wrote tdf153891: without it the reject is refused as a new violation.
    let inner = table_of(&format!("<w:tr>{}</w:tr>", cell_of(&p(&r("a")))));
    let body = |trailing: &str| {
        format!(
            "{}{}",
            table_of(&format!(
                "<w:tr>{}</w:tr>",
                cell_of(&format!("{inner}{trailing}"))
            )),
            p(&r("after"))
        )
    };
    let deleted = format!(
        "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p>",
        eric(1),
        del(2, "old")
    );
    let a = run(&body(&deleted), &[accept_all()]);
    assert!(a.xml.contains("</w:tbl><w:p/></w:tc>"), "{}", a.xml);
    let inserted = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p>",
        eric(1),
        ins(2, "new")
    );
    let j = run(&body(&inserted), &[reject_all()]);
    assert_eq!(j.xml, body("<w:p/>"));
}

#[test]
fn accepting_a_whole_paragraph_deletion_removes_its_element_and_leaves_the_next_paragraph_as_it_was()
 {
    // Design 6.2: a first paragraph with no content left goes, and the second keeps its element and properties. A
    // w:proofErr and a run with nothing but a w:lastRenderedPageBreak are no content, and a bookmark stays between blocks.
    let first = format!(
        "<w:p><w:pPr><w:jc w:val=\"center\"/><w:rPr><w:del {}/></w:rPr></w:pPr><w:bookmarkStart w:id=\"7\" w:name=\"b\"/><w:proofErr w:type=\"spellStart\"/>{}<w:proofErr w:type=\"spellEnd\"/><w:r><w:lastRenderedPageBreak/></w:r><w:bookmarkEnd w:id=\"7\"/></w:p>",
        eric(1),
        del(2, "gone")
    );
    let second = format!(
        "<w:p><w:pPr><w:jc w:val=\"right\"/></w:pPr>{}</w:p>",
        r("kept")
    );
    let a = run(&format!("{first}{second}"), &[accept_all()]);
    assert_eq!(
        a.xml,
        format!("<w:bookmarkStart w:id=\"7\" w:name=\"b\"/><w:bookmarkEnd w:id=\"7\"/>{second}")
    );
}

#[test]
fn a_join_into_a_paragraph_whose_properties_are_empty_leaves_no_empty_properties() {
    // Word writes `<w:pPr/>` now and then; the joined paragraph takes the later one's properties, and empty ones
    // are no properties.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p><w:pPr/>{}</w:p>",
        eric(1),
        r("a"),
        r("b")
    );
    let a = run(&body, &[accept_all()]);
    assert_eq!(a.xml, format!("<w:p>{}{}</w:p>", r("a"), r("b")));
    let marked = body.replace(
        "<w:pPr/>",
        "<w:pPr><w:jc w:val=\"center\"/><w:rPr></w:rPr></w:pPr>",
    );
    let a = run(&marked, &[accept_all()]);
    assert_eq!(
        a.xml,
        format!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>{}{}</w:p>",
            r("a"),
            r("b")
        )
    );
}

#[test]
fn accepting_a_deleted_row_keeps_a_table_whose_other_rows_are_in_content_controls() {
    // Strict01 wraps rows in row-level content controls: the table still has rows, so it stays; a control that held
    // only the removed row goes with it.
    let sdt = |row: &str| {
        format!(
            "<w:sdt><w:sdtPr><w:id w:val=\"7\"/></w:sdtPr><w:sdtContent>{row}</w:sdtContent></w:sdt>"
        )
    };
    let deleted = format!(
        "<w:tr><w:trPr><w:del {}/></w:trPr>{}</w:tr>",
        eric(1),
        cell_of(&format!(
            "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p>",
            eric(2),
            del(3, "gone")
        ))
    );
    let kept = format!("<w:tr>{}</w:tr>", cell_of(&p(&r("kept"))));
    for rows in [
        format!("{deleted}{}", sdt(&kept)),
        format!("{}{kept}", sdt(&deleted)),
    ] {
        let body = format!("{}{}", table_of(&rows), p(&r("after")));
        let a = run(&body, &[accept_all()]);
        assert_eq!(a.fin, ["kept", "after"], "{}", a.xml);
        assert!(a.xml.contains("<w:tbl>"), "{}", a.xml);
        assert!(
            !a.xml.contains("<w:sdtContent></w:sdtContent>") && !a.xml.contains("<w:sdtContent/>"),
            "{}",
            a.xml
        );
    }
}

#[test]
fn a_story_is_never_left_with_no_paragraph() {
    // A body that held only a table whose one row is deleted: accepting removes the table, and one empty paragraph
    // is left, which takes the comment's markers and reference run.
    let row = |inner: &str| {
        format!(
            "<w:tr><w:trPr><w:del {}/></w:trPr>{}</w:tr>",
            eric(1),
            cell_of(&format!(
                "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{inner}{}</w:p>",
                eric(2),
                del(3, "gone")
            ))
        )
    };
    let plain = run(&table_of(&row("")), &[accept_all()]);
    assert_eq!(plain.xml, "<w:p/>");
    let anchored = row(
        "<w:commentRangeStart w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r>",
    );
    let a = run(&table_of(&anchored), &[accept_all()]);
    assert_eq!(
        a.xml,
        "<w:p><w:commentRangeStart w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>"
    );
}
