//! W8 (`field_split_by_deletion`): the pieces of a field are all in a deletion or all outside one.
//! The finding belongs to the paragraph that holds the field's `begin`, so an old cut field cannot
//! hide a new cut in another paragraph, and a field cut by an edit elsewhere is found although the
//! paragraph of its `begin` did not change.

use super::tests::state;
use super::tests_edit::{
    Said, accepted, append, detach, el_with, key, nth, refused, said, say, set_attr,
    transaction_with,
};
use super::tests_units::{TABLE, para};
use super::{Declared, Origin, document};
use crate::api::check::FindingCode as C;
use crate::state::State;
use crate::xml::{NodeId, SpanTree};

const BEGIN: &str = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r>"#;
const INSTR: &str = r#"<w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r>"#;
const SEPARATE: &str = r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r>"#;
const RESULT: &str = "<w:r><w:t>1</w:t></w:r>";
const END: &str = r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>"#;

/// A paragraph with one complete field.
fn field() -> String {
    format!("<w:p>{BEGIN}{INSTR}{SEPARATE}{RESULT}{END}</w:p>")
}

/// The same field, cut: its `begin` is in a deletion that holds nothing else.
fn cut() -> String {
    format!(
        r#"<w:p><w:del w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z">{BEGIN}</w:del>{INSTR}{SEPARATE}{RESULT}{END}</w:p>"#
    )
}

/// Wraps the run in a deletion of its own.
fn delete(t: &mut SpanTree, run: NodeId, id: &str) {
    let parent = t.node_ref(run).parent().map(|p| p.id()).expect("a parent");
    let del = el_with(
        t,
        "del",
        &[
            ("id", id),
            ("author", "a"),
            ("date", "2024-01-01T00:00:00Z"),
        ],
    );
    t.edit()
        .insert_before(parent, Some(run), del)
        .expect("inserts");
    detach(t, run);
    append(t, del, run);
}

/// The nth run that holds a `w:fldChar` of this type.
fn piece(t: &SpanTree, kind: &str, n: usize) -> NodeId {
    let mut found = Vec::new();
    for id in super::tests::named(t, "fldChar") {
        if t.node_ref(id)
            .attr(super::tests::W, "fldCharType")
            .as_deref()
            == Some(kind)
        {
            found.push(t.node_ref(id).parent().map(|p| p.id()).expect("a run"));
        }
    }
    found[n]
}

fn judged(s: &State, change: impl FnOnce(&mut SpanTree)) -> Said {
    said(s, change)
}

#[test]
fn a_deletion_that_cuts_a_field_is_refused() {
    let s = state(&field());
    let cut_at_the_begin = judged(&s, |t| {
        let run = piece(t, "begin", 0);
        delete(t, run, "1");
    });
    refused(&cut_at_the_begin);
    assert_eq!(cut_at_the_begin.new, [C::FieldSplitByDeletion]);
    let cut_at_the_end = judged(&s, |t| {
        let run = piece(t, "end", 0);
        delete(t, run, "1");
    });
    refused(&cut_at_the_end);
    assert_eq!(cut_at_the_end.new, [C::FieldSplitByDeletion]);
    // a field deleted whole, piece by piece, is consistent
    let whole = judged(&s, |t| {
        for (kind, id) in [("begin", "1"), ("separate", "2"), ("end", "3")] {
            let run = piece(t, kind, 0);
            delete(t, run, id);
        }
    });
    accepted(&whole);
}

#[test]
fn a_field_that_was_cut_already_hides_another_cut_in_its_paragraph_and_not_in_another() {
    let two_in_one = format!("<w:p>{BEGIN}{SEPARATE}{END}{BEGIN}{SEPARATE}{END}</w:p>");
    let s = state(&two_in_one);
    let first = judged(&s, |t| {
        let run = piece(t, "begin", 0);
        delete(t, run, "1");
    });
    refused(&first);
    // the first field is cut; cutting the second repeats the key of the paragraph
    let old = state(&format!(
        r#"<w:p><w:del w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z">{BEGIN}</w:del>{SEPARATE}{END}{BEGIN}{SEPARATE}{END}</w:p>"#
    ));
    let again = judged(&old, |t| {
        let run = piece(t, "begin", 1);
        delete(t, run, "2");
    });
    assert!(accepted(&again).carried_total >= 1);
    // in another paragraph it is new
    let apart = state(&format!("{}{}", cut(), field()));
    let other = judged(&apart, |t| {
        let run = piece(t, "begin", 1);
        delete(t, run, "2");
    });
    refused(&other);
    assert_eq!(other.new, [C::FieldSplitByDeletion]);
}

#[test]
fn a_field_cut_from_another_paragraph_is_the_paragraph_of_its_begin() {
    // the begin and the instruction are in the first paragraph, the result and the end in the second
    let body = format!(
        "<w:p>{BEGIN}{INSTR}{SEPARATE}</w:p><w:p>{RESULT}{END}</w:p>{}",
        para("later")
    );
    let s = state(&body);
    let cut_in_the_second = judged(&s, |t| {
        let run = piece(t, "end", 0);
        delete(t, run, "1");
    });
    refused(&cut_in_the_second);
    assert_eq!(cut_in_the_second.new, [C::FieldSplitByDeletion]);
    let m = cut_in_the_second.result.expect_err("refused");
    assert!(
        m.message().contains("p1: "),
        "the first paragraph: {}",
        m.message()
    );
}

#[test]
fn a_copy_of_a_paragraph_with_a_cut_field_declares_where_it_came_from() {
    let s = state(&cut());
    let copy = |t: &mut SpanTree| {
        let p = nth(t, "p", 0);
        let copy = t.edit().clone_subtree(p).expect("clones");
        let body = nth(t, "body", 0);
        let sect = nth(t, "sectPr", 0);
        t.edit()
            .insert_before(body, Some(sect), copy)
            .expect("inserts");
        // the revision in the copy needs an id of its own
        let del = nth(t, "del", 1);
        set_attr(t, del, "id", "9");
        (p, copy)
    };
    let (tx, (p, c)) = transaction_with(&s, copy);
    let declared = Declared {
        origins: vec![Origin {
            block: key(c),
            from: vec![key(p)],
        }],
        foreign: Vec::new(),
        parts: Vec::new(),
    };
    accepted(&say(&tx, &declared));
    let undeclared = say(&tx, &Declared::default());
    refused(&undeclared);
    assert!(undeclared.new.contains(&C::FieldSplitByDeletion));
}

#[test]
fn a_simple_field_has_one_piece_and_is_never_cut() {
    let s = state(
        r#"<w:p><w:fldSimple w:instr=" PAGE "><w:r><w:delText>1</w:delText></w:r></w:fldSimple></w:p>"#,
    );
    let delete_the_run = judged(&s, |t| {
        let run = nth(t, "r", 0);
        delete(t, run, "1");
    });
    accepted(&delete_the_run);
    // fields in a table cell are read as everywhere
    let s = state(&format!("{TABLE}{}", field()));
    let cut_in_the_body = judged(&s, |t| {
        let run = piece(t, "begin", 0);
        delete(t, run, "1");
    });
    refused(&cut_in_the_body);
}

#[test]
fn a_report_of_the_document_lists_a_cut_field_in_the_paragraph_of_its_begin() {
    let s = state(&format!("{}{}", para("one"), cut()));
    let report = document(&s.parts());
    let found: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.code == C::FieldSplitByDeletion)
        .collect();
    assert_eq!(found.len(), 1, "{report:?}");
    assert_eq!(found[0].block.as_deref(), Some("p2"));
    assert!(!report.ok);
    // a document with a complete field has no such finding
    let clean = state(&field());
    assert!(document(&clean.parts()).ok);
}
