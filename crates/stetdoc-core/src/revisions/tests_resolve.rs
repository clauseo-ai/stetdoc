//! Known answers for accept and reject: one test for each row of table 3B of the design, on XML written by
//! hand.

use super::index::ChangeIndex;
use super::resolve::{Resolution, resolve};
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::wordml::Mode;

pub(super) fn accept_all() -> Step {
    Step {
        action: Action::Accept,
        selector: Selector::All,
    }
}

pub(super) fn reject_all() -> Step {
    Step {
        action: Action::Reject,
        selector: Selector::All,
    }
}

pub(super) struct Out {
    pub(super) xml: String,
    pub(super) fin: Vec<String>,
    pub(super) orig: Vec<String>,
    pub(super) res: Vec<Resolution>,
}

/// Resolves the steps over a body and reads the result.
pub(super) fn run(body: &str, steps: &[Step]) -> Out {
    let s = state(body);
    let mut tx = s.begin();
    let mut index = ChangeIndex::new();
    let res = resolve(&mut tx, &mut index, steps).unwrap_or_else(|e| panic!("resolve failed: {e}"));
    let added = schema_added(&s, &tx);
    assert!(
        added.is_empty(),
        "new schema violations {added:?}: {}",
        body_xml(&tx)
    );
    let xml = body_xml(&tx);
    let fin = run_texts(&tx.parts(), Mode::Final);
    let orig = run_texts(&tx.parts(), Mode::Original);
    tx.commit()
        .unwrap_or_else(|e| panic!("commit failed: {e}\n{xml}"));
    Out {
        xml,
        fin,
        orig,
        res,
    }
}

pub(super) fn ins(id: u32, text: &str) -> String {
    format!("<w:ins {}>{}</w:ins>", eric(id), r(text))
}

pub(super) fn del(id: u32, text: &str) -> String {
    format!(
        "<w:del {}><w:r><w:delText xml:space=\"preserve\">{text}</w:delText></w:r></w:del>",
        eric(id)
    )
}

#[test]
fn accepting_an_insertion_unwraps_it_and_rejecting_removes_it() {
    let body = p(&format!("{}{}{}", r("a "), ins(1, "new"), r(" b")));
    let a = run(&body, &[accept_all()]);
    assert_eq!(a.xml, p(&format!("{}{}{}", r("a "), r("new"), r(" b"))));
    let j = run(&body, &[reject_all()]);
    assert_eq!(j.xml, p(&format!("{}{}", r("a "), r(" b"))));
}

#[test]
fn accepting_a_deletion_removes_it_and_rejecting_unwraps_it_with_the_text_renamed_back() {
    let body = p(&format!("{}{}{}", r("a "), del(1, "old"), r(" b")));
    assert_eq!(
        run(&body, &[accept_all()]).xml,
        p(&format!("{}{}", r("a "), r(" b")))
    );
    let j = run(&body, &[reject_all()]);
    assert_eq!(
        j.xml,
        p(&format!(
            "{}<w:r><w:t xml:space=\"preserve\">old</w:t></w:r>{}",
            r("a "),
            r(" b")
        ))
    );
}

#[test]
fn a_deletion_nested_in_an_insertion_is_resolved_inside_out() {
    let body = p(&format!(
        "<w:ins {}>{}<w:del {}><w:r><w:delText>gone</w:delText></w:r></w:del>{}</w:ins>",
        eric(1),
        r("keep "),
        eric(2),
        r(" end")
    ));
    assert_eq!(
        run(&body, &[accept_all()]).xml,
        p(&format!("{}{}", r("keep "), r(" end")))
    );
    assert_eq!(run(&body, &[reject_all()]).xml, p(""));
    // Accepting only the insertion leaves the other author's deletion pending.
    let only = run(
        &body,
        &[Step {
            action: Action::Accept,
            selector: Selector::Elements(vec![1]),
        }],
    );
    assert!(
        only.xml.contains("<w:del ") && !only.xml.contains("<w:ins "),
        "{}",
        only.xml
    );
    // Rejecting only the insertion takes the deletion with it and says so.
    let gone = run(
        &body,
        &[Step {
            action: Action::Reject,
            selector: Selector::Elements(vec![1]),
        }],
    );
    assert_eq!(gone.res[0].swallowed, [2]);
    assert!(!gone.xml.contains("w:del"), "{}", gone.xml);
}

#[test]
fn rejecting_a_deletion_inside_an_insertion_renames_its_text_back() {
    let body = p(&format!(
        "<w:ins {}><w:del {}><w:r><w:delText>x</w:delText></w:r></w:del></w:ins>",
        eric(1),
        eric(2)
    ));
    let s = run(
        &body,
        &[Step {
            action: Action::Reject,
            selector: Selector::Elements(vec![2]),
        }],
    );
    assert!(
        s.xml.contains("<w:ins ") && s.xml.contains("<w:t>x</w:t>") && !s.xml.contains("delText"),
        "{}",
        s.xml
    );
}

#[test]
fn range_markers_in_removed_content_are_hoisted() {
    let body = p(&format!(
        "{}<w:ins {}><w:commentRangeStart w:id=\"3\"/>{}<w:commentRangeEnd w:id=\"3\"/><w:r><w:commentReference w:id=\"3\"/></w:r></w:ins>",
        r("a"),
        eric(1),
        r("inside")
    ));
    let s = run(&body, &[reject_all()]);
    assert_eq!(
        s.xml,
        p(&format!(
            "{}<w:commentRangeStart w:id=\"3\"/><w:commentRangeEnd w:id=\"3\"/><w:r><w:commentReference w:id=\"3\"/></w:r>",
            r("a")
        ))
    );
}

#[test]
fn a_mark_revision_that_does_not_remove_the_mark_is_just_removed() {
    let ins_mark = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        eric(1),
        r("a"),
        r("b")
    );
    let a = run(&ins_mark, &[accept_all()]);
    assert_eq!(a.xml, format!("<w:p>{}</w:p><w:p>{}</w:p>", r("a"), r("b")));
    let del_mark = format!(
        "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        eric(1),
        r("a"),
        r("b")
    );
    let j = run(&del_mark, &[reject_all()]);
    assert_eq!(j.xml, format!("<w:p>{}</w:p><w:p>{}</w:p>", r("a"), r("b")));
}

#[test]
fn a_deleted_mark_that_is_accepted_joins_the_paragraph_with_the_next_and_keeps_the_later_properties()
 {
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"A\"/><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p><w:pPr><w:pStyle w:val=\"B\"/></w:pPr>{}</w:p>",
        eric(1),
        r("one "),
        r("two")
    );
    let s = run(&body, &[accept_all()]);
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"B\"/></w:pPr>{}{}</w:p>",
            r("one "),
            r("two")
        )
    );
    assert_eq!(s.res[0].joined.len(), 1);
}

#[test]
fn an_inserted_mark_that_is_rejected_joins_the_halves_and_a_whole_inserted_paragraph_goes() {
    // Y: the first half has the inserted mark, the second holds the original mark and inserted text.
    let y = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p><w:ins {}>{}</w:ins></w:p><w:p>{}</w:p>",
        eric(1),
        r("keep"),
        eric(2),
        r("new text"),
        r("next")
    );
    let s = run(&y, &[reject_all()]);
    assert_eq!(
        s.xml,
        format!("<w:p>{}</w:p><w:p>{}</w:p>", r("keep"), r("next"))
    );
    // X: an inserted paragraph before another goes whole.
    let x = format!(
        "<w:p>{}</w:p><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}>{}</w:ins></w:p><w:p>{}</w:p>",
        r("before"),
        eric(1),
        eric(2),
        r("new"),
        r("after")
    );
    let s = run(&x, &[reject_all()]);
    assert_eq!(
        s.xml,
        format!("<w:p>{}</w:p><w:p>{}</w:p>", r("before"), r("after"))
    );
}

#[test]
fn a_run_of_deleted_marks_collapses_into_the_first_paragraph_with_the_last_properties() {
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"A\"/><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"B\"/><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"C\"/></w:pPr>{}</w:p>",
        eric(1),
        r("1"),
        eric(2),
        r("2"),
        r("3")
    );
    let s = run(&body, &[accept_all()]);
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"C\"/></w:pPr>{}{}{}</w:p>",
            r("1"),
            r("2"),
            r("3")
        )
    );
}

#[test]
fn a_whole_paragraph_deletion_that_is_accepted_leaves_the_next_paragraph_alone() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p><w:pPr><w:pStyle w:val=\"B\"/></w:pPr>{}</w:p>",
        eric(1),
        del(2, "gone"),
        r("next")
    );
    let s = run(&body, &[accept_all()]);
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"B\"/></w:pPr>{}</w:p>",
            r("next")
        )
    );
}

#[test]
fn a_deleted_mark_with_nothing_after_it_is_dropped_and_an_empty_paragraph_goes() {
    let last = format!(
        "<w:p>{}</w:p><w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p>",
        r("a"),
        eric(1),
        r("tail")
    );
    let s = run(&last, &[accept_all()]);
    assert_eq!(
        s.xml,
        format!("<w:p>{}</w:p><w:p>{}</w:p>", r("a"), r("tail"))
    );
    let empty = format!(
        "<w:p>{}</w:p><w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr></w:p>",
        r("a"),
        eric(1)
    );
    let e = run(&empty, &[accept_all()]);
    assert_eq!(e.xml, format!("<w:p>{}</w:p>", r("a")));
}

#[test]
fn a_paragraph_property_change_is_removed_on_accept_and_its_snapshot_comes_back_on_reject() {
    let body = format!(
        "<w:p><w:pPr><w:jc w:val=\"right\"/><w:rPr><w:b/></w:rPr><w:pPrChange {}><w:pPr><w:pStyle w:val=\"Old\"/></w:pPr></w:pPrChange></w:pPr>{}</w:p>",
        eric(1),
        r("t")
    );
    let a = run(&body, &[accept_all()]);
    assert_eq!(
        a.xml,
        format!(
            "<w:p><w:pPr><w:jc w:val=\"right\"/><w:rPr><w:b/></w:rPr></w:pPr>{}</w:p>",
            r("t")
        )
    );
    let j = run(&body, &[reject_all()]);
    assert_eq!(
        j.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Old\"/><w:rPr><w:b/></w:rPr></w:pPr>{}</w:p>",
            r("t")
        )
    );
}

#[test]
fn a_run_property_change_with_an_empty_snapshot_leaves_no_rpr_after_a_reject() {
    let body = p(&format!(
        "<w:r><w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr><w:t>x</w:t></w:r>",
        eric(1)
    ));
    assert_eq!(
        run(&body, &[reject_all()]).xml,
        p("<w:r><w:t>x</w:t></w:r>")
    );
    assert_eq!(
        run(&body, &[accept_all()]).xml,
        p("<w:r><w:rPr><w:b/></w:rPr><w:t>x</w:t></w:r>")
    );
}

#[test]
fn rejecting_a_section_change_keeps_the_header_references() {
    let body = format!(
        "<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:sectPrChange {}><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:sectPrChange></w:sectPr></w:pPr>{}</w:p>",
        eric(1),
        r("end")
    );
    let j = run(&body, &[reject_all()]);
    assert!(
        j.xml.contains("<w:headerReference") && j.xml.contains("w:w=\"12240\""),
        "{}",
        j.xml
    );
    assert!(
        !j.xml.contains("11906") && !j.xml.contains("sectPrChange"),
        "{}",
        j.xml
    );
}

#[test]
fn a_mark_that_has_both_an_insertion_and_a_deletion_resolves_by_each_in_turn() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/><w:del w:id=\"2\" w:author=\"Fay\" w:date=\"2021-01-01T00:00:00Z\"/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        eric(1),
        r("a"),
        r("b")
    );
    // Accepting Eric's insertion leaves Fay's deletion pending.
    let s = run(
        &body,
        &[Step {
            action: Action::Accept,
            selector: Selector::Author("Eric".into()),
        }],
    );
    assert!(
        s.xml.contains("<w:del ") && !s.xml.contains("<w:ins "),
        "{}",
        s.xml
    );
    // Accepting everything joins.
    let all = run(&body, &[accept_all()]);
    assert_eq!(all.xml, format!("<w:p>{}{}</w:p>", r("a"), r("b")));
}
