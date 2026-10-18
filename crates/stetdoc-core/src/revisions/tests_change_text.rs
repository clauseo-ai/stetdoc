//! What a change carries of the text it puts in and takes out (`Change::inserted`, `deleted`, `text_cut`),
//! which a receipt shows and a gate on what an agent wrote reads.

use super::changes::{CHANGE_TEXT_LIMIT, Change, ChangeKind, changes};
use super::tests_support::*;

fn a(id: u32) -> String {
    format!("w:id=\"{id}\" w:author=\"Ann\" w:date=\"2026-01-01T00:00:00Z\"")
}

fn b(id: u32) -> String {
    format!("w:id=\"{id}\" w:author=\"Bob\" w:date=\"2026-01-02T00:00:00Z\"")
}

fn of(body: &str) -> Vec<Change> {
    let s = state(body);
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    changes(&view.story(id).unwrap(), view.tree(id).unwrap(), None)
}

fn del_run(text: &str) -> String {
    format!("<w:r><w:delText xml:space=\"preserve\">{text}</w:delText></w:r>")
}

#[test]
fn an_insertion_carries_its_text_and_a_deletion_the_text_it_took_out() {
    let body = p(&format!(
        "{}<w:del {}>{}</w:del><w:ins {}>{}</w:ins>",
        r("keep "),
        a(1),
        del_run("old words"),
        a(2),
        r("new words")
    ));
    let cs = of(&body);
    assert_eq!(cs.len(), 2);
    let (d, i) = (&cs[0], &cs[1]);
    assert_eq!(
        (d.kind, d.deleted.as_str(), d.inserted.as_str()),
        (ChangeKind::Deletion, "old words", "")
    );
    assert_eq!(
        (i.kind, i.inserted.as_str(), i.deleted.as_str()),
        (ChangeKind::Insertion, "new words", "")
    );
    assert!(!d.text_cut && !i.text_cut);
}

#[test]
fn tabs_and_breaks_are_the_characters_they_are_and_a_field_code_is_not_text() {
    let body = p(&format!(
        "<w:ins {}><w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t></w:r><w:r><w:instrText> PAGE </w:instrText></w:r></w:ins>",
        a(1)
    ));
    assert_eq!(of(&body)[0].inserted, "a\tb\nc");
}

#[test]
fn text_that_another_author_deleted_inside_an_insertion_is_not_what_the_insertion_holds() {
    let body = p(&format!(
        "<w:ins {}>{}<w:del {}>{}</w:del>{}</w:ins>",
        a(1),
        r("keep "),
        b(2),
        del_run("gone "),
        r("end")
    ));
    let cs = of(&body);
    let ins = cs.iter().find(|c| c.kind == ChangeKind::Insertion).unwrap();
    let del = cs.iter().find(|c| c.kind == ChangeKind::Deletion).unwrap();
    assert_eq!(ins.inserted, "keep end");
    assert_eq!(del.deleted, "gone ");
}

#[test]
fn an_inserted_paragraph_carries_its_text_and_a_break_for_each_inserted_mark() {
    let mark = |id: u32| format!("<w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>", a(id));
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}<w:ins {}>{}</w:ins></w:p><w:p>{}</w:p>",
        mark(1),
        r("one"),
        mark(3),
        a(2),
        r("two"),
        r("three")
    );
    let cs = of(&body);
    assert_eq!(cs.len(), 1, "{cs:?}");
    assert_eq!(cs[0].kind, ChangeKind::ParagraphInserted);
    assert_eq!(cs[0].inserted, "\ntwo\n");
}

#[test]
fn the_text_of_a_change_is_cut_at_the_limit_and_says_so() {
    let long = "x".repeat(CHANGE_TEXT_LIMIT + 500);
    let body = p(&format!("<w:ins {}>{}</w:ins>", a(1), r(&long)));
    let cs = of(&body);
    assert_eq!(cs[0].inserted.chars().count(), CHANGE_TEXT_LIMIT);
    assert!(cs[0].text_cut);
    // Whole characters: a multi-byte one is not cut in two.
    let wide = "\u{e9}".repeat(CHANGE_TEXT_LIMIT + 1);
    let body = p(&format!("<w:del {}>{}</w:del>", a(1), del_run(&wide)));
    let cs = of(&body);
    assert_eq!(cs[0].deleted.chars().count(), CHANGE_TEXT_LIMIT);
    assert!(cs[0].text_cut);
}

#[test]
fn the_text_of_an_inserted_row_is_the_text_of_its_cells() {
    let cell = |t: &str, id: u32| {
        format!(
            "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}>{}</w:ins></w:p></w:tc>",
            a(id),
            a(id + 1),
            r(t)
        )
    };
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:trPr><w:ins {}/></w:trPr>{}{}</w:tr></w:tbl>{}",
        a(9),
        cell("left", 10),
        cell("right", 20),
        p(&r("after"))
    );
    let cs = of(&body);
    let row = cs
        .iter()
        .find(|c| c.kind == ChangeKind::RowInserted)
        .unwrap();
    assert!(
        row.inserted.contains("left") && row.inserted.contains("right"),
        "{:?}",
        row.inserted
    );
}
