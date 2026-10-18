//! Known answers for accept and reject, continued: tables, fields, selectors and batches.

use super::index::ChangeIndex;
use super::resolve::resolve;
use super::tests_resolve::{accept_all, del, ins, reject_all, run};
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

// --- tables

const TBL: &str =
    "<w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>";

fn tbl(rows: &str) -> String {
    format!("<w:tbl>{TBL}{rows}</w:tbl><w:p>{}</w:p>", r("after"))
}

#[test]
fn an_inserted_row_loses_its_marker_on_accept_and_goes_on_reject() {
    let rows = format!(
        "<w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p/></w:tc></w:tr><w:tr><w:trPr><w:ins {}/></w:trPr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p/></w:tc></w:tr>",
        r("keep"),
        eric(1),
        r("new")
    );
    let a = run(&tbl(&rows), &[accept_all()]);
    assert_eq!(a.xml.matches("<w:tr>").count(), 2, "{}", a.xml);
    assert!(!a.xml.contains("trPr"), "{}", a.xml);
    let j = run(&tbl(&rows), &[reject_all()]);
    assert_eq!(j.xml.matches("<w:tr>").count(), 1, "{}", j.xml);
    assert!(j.res[0].removed.iter().any(|b| b.rejected));
}

#[test]
fn a_deleted_row_goes_on_accept_and_keeps_its_content_on_reject() {
    let rows = format!(
        "<w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p/></w:tc></w:tr><w:tr><w:trPr><w:del {}/></w:trPr><w:tc><w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p></w:tc><w:tc><w:p/></w:tc></w:tr>",
        r("keep"),
        eric(1),
        eric(2),
        del(3, "gone")
    );
    let a = run(&tbl(&rows), &[accept_all()]);
    assert_eq!(a.xml.matches("<w:tr>").count(), 1, "{}", a.xml);
    let j = run(&tbl(&rows), &[reject_all()]);
    assert_eq!(j.xml.matches("<w:tr>").count(), 2, "{}", j.xml);
    assert!(j.xml.contains("gone</w:t>"), "{}", j.xml);
    assert!(!j.xml.contains("w:del"), "{}", j.xml);
}

#[test]
fn a_table_whose_every_row_goes_is_removed() {
    let rows = format!(
        "<w:tr><w:trPr><w:del {}/></w:trPr><w:tc><w:p/></w:tc></w:tr>",
        eric(1)
    );
    let a = run(&tbl(&rows), &[accept_all()]);
    assert!(!a.xml.contains("<w:tbl>"), "{}", a.xml);
    assert_eq!(a.fin, ["after"]);
}

#[test]
fn an_accepted_cell_deletion_widens_the_cell_before_it() {
    let rows = format!(
        "<w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"1\"/></w:tcPr><w:p>{}</w:p></w:tc><w:tc><w:tcPr><w:cellDel {}/></w:tcPr><w:p/></w:tc></w:tr>",
        r("a"),
        eric(1)
    );
    let a = run(&tbl(&rows), &[accept_all()]);
    assert_eq!(a.xml.matches("<w:tc>").count(), 1, "{}", a.xml);
    assert!(a.xml.contains("<w:gridSpan w:val=\"2\"/>"), "{}", a.xml);
}

#[test]
fn a_cell_merge_becomes_a_vertical_merge_on_accept() {
    let rows = format!(
        "<w:tr><w:tc><w:tcPr><w:cellMerge {} w:vMerge=\"rest\"/></w:tcPr><w:p>{}</w:p></w:tc><w:tc><w:p/></w:tc></w:tr>",
        eric(1),
        r("a")
    );
    let a = run(&tbl(&rows), &[accept_all()]);
    assert!(
        a.xml.contains("<w:vMerge w:val=\"restart\"/>") && !a.xml.contains("cellMerge"),
        "{}",
        a.xml
    );
}

// --- fields

fn field(code: &str, result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r(result)
    )
}

#[test]
fn rejecting_an_inserted_field_removes_every_piece_and_the_unwrapped_separator() {
    // Word writes ins{begin, code}, the separator with no wrapper, ins{result, end}.
    let body = p(&format!(
        "{}<w:ins {}><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r></w:ins><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:ins {}>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:ins>{}",
        r("a "),
        eric(1),
        eric(2),
        r("3"),
        r(" b")
    ));
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.xml, p(&format!("{}{}", r("a "), r(" b"))));
    let a = run(&body, &[accept_all()]);
    assert_eq!(a.xml.matches("<w:fldChar ").count(), 3, "{}", a.xml);
    assert!(!a.xml.contains("<w:ins "), "{}", a.xml);
}

#[test]
fn accepting_a_deleted_field_removes_it_and_rejecting_restores_the_code_names() {
    let body = p(&format!(
        "{}<w:del {}><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:delInstrText> PAGE </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:delText>3</w:delText></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:del>{}",
        r("a "),
        eric(1),
        r(" b")
    ));
    assert_eq!(
        run(&body, &[accept_all()]).xml,
        p(&format!("{}{}", r("a "), r(" b")))
    );
    let j = run(&body, &[reject_all()]);
    assert!(
        j.xml.contains("<w:instrText> PAGE </w:instrText>") && j.xml.contains("<w:t>3</w:t>"),
        "{}",
        j.xml
    );
}

const BEGIN: &str = "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>";
const SEPARATE: &str = "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>";
const END: &str = "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";

#[test]
fn a_revision_removes_what_it_holds_of_a_field_and_not_the_result_it_does_not_hold() {
    // tdf124472: only the code of a field was replaced. Accepting the deletion of the old code, and
    // rejecting the insertion of the new, take the code and nothing else: the field and its result stay.
    let body = p(&format!(
        "{BEGIN}<w:ins {}><w:r><w:instrText>HYPERLINK \"new\"</w:instrText></w:r></w:ins><w:del {}><w:r><w:delInstrText>HYPERLINK \"old\"</w:delInstrText></w:r></w:del>{SEPARATE}{}{END}",
        eric(1),
        eric(2),
        r("Libreoffice")
    ));
    let a = run(&body, &[accept_all()]);
    assert_eq!(a.xml.matches("<w:fldChar ").count(), 3, "{}", a.xml);
    assert!(
        a.xml.contains("HYPERLINK \"new\"") && !a.xml.contains("old"),
        "{}",
        a.xml
    );
    assert!(a.xml.contains(">Libreoffice</w:t>"), "{}", a.xml);
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.xml.matches("<w:fldChar ").count(), 3, "{}", j.xml);
    assert!(
        j.xml
            .contains("<w:instrText>HYPERLINK \"old\"</w:instrText>"),
        "{}",
        j.xml
    );
    assert!(j.xml.contains(">Libreoffice</w:t>"), "{}", j.xml);
}

#[test]
fn rejecting_an_inserted_field_whose_result_is_outside_the_insertion_keeps_the_result_as_text() {
    // stress023: ins{begin, code, separate}, the result with no wrapper, ins{end}.
    let body = p(&format!(
        "<w:ins {}><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r>{SEPARATE}</w:ins>{}<w:ins {}>{END}</w:ins>{}",
        eric(1),
        r("1"),
        eric(2),
        r(" - end")
    ));
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.xml, p(&format!("{}{}", r("1"), r(" - end"))));
    assert_eq!(j.fin, ["1 - end"]);
}

#[test]
fn rejecting_the_insertion_of_the_start_and_the_end_of_a_field_leaves_the_result_and_no_orphan_piece()
 {
    // Academic_Regulations: ins{begin, code}, the separator and the result with no wrapper, ins{end}.
    let body = p(&format!(
        "<w:ins {}>{BEGIN}<w:r><w:instrText> STYLEREF Titel </w:instrText></w:r></w:ins>{SEPARATE}{}<w:ins {}>{END}</w:ins>",
        eric(1),
        r("Notes"),
        eric(2)
    ));
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.xml, p(&r("Notes")));
    let a = run(&body, &[accept_all()]);
    assert_eq!(a.xml.matches("<w:fldChar ").count(), 3, "{}", a.xml);
    assert!(a.xml.contains(">Notes</w:t>"), "{}", a.xml);
}

#[test]
fn a_join_does_not_reach_across_a_table_that_the_step_removes() {
    // RP051-Arabic: a paragraph with an inserted mark, an empty one with an inserted mark, an inserted table,
    // and a paragraph. The text model reads the paragraph before the table and the one after as two, and so
    // does the resolution: rejecting everything leaves "one" and "three" as two paragraphs.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p>\
         <w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr></w:p>\
         <w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:trPr><w:ins {}/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}>{}</w:ins></w:p></w:tc></w:tr></w:tbl>\
         <w:p>{}</w:p>",
        eric(1),
        r("one"),
        eric(2),
        eric(3),
        eric(4),
        eric(5),
        r("cell"),
        r("three")
    );
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.fin, ["one", "three"], "{}", j.xml);
    assert!(
        !j.xml.contains("<w:tbl>") && !j.xml.contains("<w:ins "),
        "{}",
        j.xml
    );
}

#[test]
fn a_marker_between_blocks_stays_after_the_markers_of_the_paragraph_that_goes() {
    // RP015, tdf149711: a paragraph that holds only the start of a bookmark and an inserted mark, and the end
    // of that bookmark standing between the blocks after it. Rejecting the mark removes the paragraph, and the
    // start must not end up after the end.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:bookmarkStart w:id=\"4\" w:name=\"b\"/></w:p><w:bookmarkEnd w:id=\"4\"/><w:p>{}</w:p>",
        eric(1),
        r("text")
    );
    let j = run(&body, &[reject_all()]);
    let (start, end) = (
        j.xml.find("<w:bookmarkStart").unwrap(),
        j.xml.find("<w:bookmarkEnd").unwrap(),
    );
    assert!(start < end, "{}", j.xml);
    assert_eq!(j.fin, ["text"], "{}", j.xml);
    // The same with the leftovers going back to the paragraph before, when nothing follows.
    let body = format!(
        "<w:p>{}</w:p><w:bookmarkStart w:id=\"5\" w:name=\"c\"/><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:bookmarkEnd w:id=\"5\"/></w:p>",
        r("text"),
        eric(2)
    );
    let j = run(&body, &[reject_all()]);
    let (start, end) = (
        j.xml.find("<w:bookmarkStart").unwrap(),
        j.xml.find("<w:bookmarkEnd").unwrap(),
    );
    assert!(start < end, "{}", j.xml);
}

// --- selectors and batches

#[test]
fn a_selector_by_author_by_id_and_by_element_picks_what_it_names() {
    let body = p(&format!(
        "{}<w:ins w:id=\"2\" w:author=\"Fay\" w:date=\"2021-01-01T00:00:00Z\">{}</w:ins>",
        ins(1, "eric"),
        r("fay")
    ));
    let by_author = run(
        &body,
        &[Step {
            action: Action::Accept,
            selector: Selector::Author("Eric".into()),
        }],
    );
    assert_eq!(by_author.xml.matches("<w:ins ").count(), 1);
    let by_id = run(
        &body,
        &[Step {
            action: Action::Reject,
            selector: Selector::Ids(vec![2]),
        }],
    );
    assert!(
        !by_id.xml.contains("fay") && by_id.xml.contains("eric"),
        "{}",
        by_id.xml
    );
    let s = state(&body);
    let mut tx = s.begin();
    let mut index = ChangeIndex::new();
    let missing = resolve(
        &mut tx,
        &mut index,
        &[Step {
            action: Action::Accept,
            selector: Selector::Ids(vec![99]),
        }],
    );
    assert_eq!(missing.unwrap_err().code(), ErrorCode::NotFound);
}

#[test]
fn resolving_twice_changes_nothing_and_leaves_no_revision() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}{}</w:p><w:p>{}{}</w:p>",
        eric(1),
        ins(2, "x"),
        del(3, "y"),
        r("z"),
        del(4, "w")
    );
    for step in [accept_all(), reject_all()] {
        let once = run(&body, std::slice::from_ref(&step));
        let twice = run(&body, &[step.clone(), step.clone()]);
        assert_eq!(once.xml, twice.xml);
        assert!(
            !once.xml.contains("<w:ins ")
                && !once.xml.contains("<w:del ")
                && !once.xml.contains("Change"),
            "{}",
            once.xml
        );
    }
}

#[test]
fn rejecting_everything_and_then_editing_keeps_the_edit() {
    let s = state(&p(&format!("{}{}", r("a "), ins(1, "theirs"))));
    let mut tx = s.begin();
    let mut index = ChangeIndex::new();
    resolve(&mut tx, &mut index, &[reject_all()]).unwrap();
    let ks = para_keys(&tx);
    let mut c = tracked(&mut tx);
    c.replace_text(&ks[0], 2..2, "mine").unwrap();
    c.finish().unwrap();
    assert_eq!(texts(&tx, Mode::Final), ["a mine"]);
    assert_eq!(texts(&tx, Mode::Original), ["a "]);
}

#[test]
fn a_cell_whose_only_paragraph_is_rejected_keeps_it_empty_and_a_cell_does_not_end_with_a_table() {
    let inserted_empty = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr></w:p>",
        eric(1)
    );
    // The only paragraph of a cell is an inserted one with nothing in it: it ends the cell, so rejecting keeps it (3.2.1).
    let alone = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc>{inserted_empty}</w:tc></w:tr></w:tbl><w:p>{}</w:p>",
        r("after")
    );
    let j = run(&alone, &[reject_all()]);
    assert!(j.xml.contains("<w:tc><w:p></w:p></w:tc>"), "{}", j.xml);
    // A nested table is followed by the inserted paragraph, which a cell must not lose to end with it.
    let nested = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc>\
         <w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1000\"/></w:tblGrid><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl>{inserted_empty}\
         </w:tc></w:tr></w:tbl><w:p>{}</w:p>",
        r("inner"),
        r("after")
    );
    let j = run(&nested, &[reject_all()]);
    assert!(j.xml.contains("</w:tbl><w:p/></w:tc>"), "{}", j.xml);
}

fn control(kind: &str, id: u32) -> String {
    format!(
        "<w:customXml{kind}RangeStart {}/><w:sdt><w:sdtPr><w:alias w:val=\"c\"/></w:sdtPr><w:sdtContent><w:p>{}</w:p></w:sdtContent></w:sdt><w:customXml{kind}RangeEnd w:id=\"{}\"/>",
        eric(id),
        r("inside"),
        id
    )
}

#[test]
fn the_markers_around_an_inserted_control_go_on_accept_and_the_control_goes_on_reject_with_its_content_kept()
 {
    let body = format!("{}{}", p(&r("before")), control("Ins", 1));
    let a = run(&body, &[accept_all()]);
    assert!(
        a.xml.contains("<w:sdt>") && !a.xml.contains("customXml"),
        "{}",
        a.xml
    );
    let j = run(&body, &[reject_all()]);
    assert!(
        !j.xml.contains("<w:sdt") && !j.xml.contains("customXml"),
        "{}",
        j.xml
    );
    assert_eq!(j.fin, ["before", "inside"]);
}

#[test]
fn the_control_of_a_deleted_one_is_unwrapped_on_accept_and_kept_on_reject() {
    let body = format!("{}{}", p(&r("before")), control("Del", 1));
    let a = run(&body, &[accept_all()]);
    assert!(
        !a.xml.contains("<w:sdt") && !a.xml.contains("customXml"),
        "{}",
        a.xml
    );
    assert_eq!(a.fin, ["before", "inside"]);
    let j = run(&body, &[reject_all()]);
    assert!(
        j.xml.contains("<w:sdt>") && !j.xml.contains("customXml"),
        "{}",
        j.xml
    );
}

// --- touching

#[test]
fn a_block_whose_wrapper_a_resolution_removes_is_still_touched() {
    // Accepting the deletion inside another author's insertion leaves that insertion with nothing in it,
    // and it goes. The paragraph that held it changed, and the transaction is told so, though the node
    // that was marked as changed is no longer in the tree.
    let body = format!(
        "<w:p>{}</w:p><w:p><w:ins {}><w:del {}><w:r><w:delText>gone</w:delText></w:r></w:del></w:ins></w:p>",
        r("keep"),
        eric(1),
        eric(2)
    );
    let s = state(&body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[Step {
            action: Action::Accept,
            selector: Selector::Ids(vec![2]),
        }],
    )
    .unwrap();
    assert!(!body_xml(&tx).contains("w:ins"), "{}", body_xml(&tx));
    assert_eq!(tx.touched(), [ks[1].clone()]);
}

#[test]
fn a_bookmark_that_starts_in_a_table_that_the_step_removes_keeps_its_start() {
    // TC-table-converttotable: text converted to a table, with every row inserted, and a bookmark that starts in the
    // first cell and ends after the table. Rejecting everything removes the rows and then the table, and the start
    // comes out of both, before the end.
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:trPr><w:ins {}/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:bookmarkStart w:id=\"9\" w:name=\"_GoBack\"/><w:ins {}>{}</w:ins></w:p></w:tc></w:tr></w:tbl>\
         <w:bookmarkEnd w:id=\"9\"/><w:p>{}</w:p>",
        eric(1),
        eric(2),
        eric(3),
        r("cell"),
        r("after")
    );
    let j = run(&body, &[reject_all()]);
    assert!(!j.xml.contains("<w:tbl>"), "{}", j.xml);
    let start = j.xml.find("<w:bookmarkStart w:id=\"9\"").expect(&j.xml);
    let end = j.xml.find("<w:bookmarkEnd w:id=\"9\"").expect(&j.xml);
    assert!(start < end, "{}", j.xml);
}

#[test]
fn a_deleted_paragraph_that_took_the_end_mark_comes_back_whole_across_an_inserted_table() {
    // What compare writes when a body's content is replaced: the deleted copy of the original takes the inserted mark beside
    // its deleted one, so that the body's last mark stays live (3.2.1). Rejecting removes the inserted table first and then
    // joins the run into the last paragraph, which gives the original paragraph back alone.
    let a = |id: u32| format!(r#"w:id="{id}" w:author="E" w:date="2026-01-01T00:00:00Z""#);
    let ip = |id: u32, t: &str| {
        format!(
            r#"<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}><w:r><w:t>{t}</w:t></w:r></w:ins></w:p>"#,
            a(id),
            a(id + 1)
        )
    };
    let body = format!(
        r#"<w:p><w:pPr><w:rPr><w:ins {}/><w:del {}/></w:rPr></w:pPr><w:del {}><w:r><w:delText>old</w:delText></w:r></w:del></w:p>{}<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:trPr><w:ins {}/></w:trPr><w:tc>{}</w:tc></w:tr></w:tbl>{}<w:p><w:ins {}><w:r><w:t>p3</w:t></w:r></w:ins></w:p>"#,
        a(20),
        a(1),
        a(2),
        ip(3, "p1"),
        a(5),
        ip(6, "cell"),
        ip(8, "p2"),
        a(10)
    );
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.fin, ["old"], "{}", j.xml);
}
