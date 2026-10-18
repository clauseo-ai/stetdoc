//! Known answers for the grouping of revision elements into changes (design `revisions.md`, 6.1).

use super::changes::{Change, ChangeKind, changes};
use super::tests_support::*;
use crate::wordml::Branch;

/// A revision's attributes.
fn rev(id: u32, author: &str, date: &str) -> String {
    format!("w:id=\"{id}\" w:author=\"{author}\" w:date=\"{date}\"")
}

fn a(id: u32) -> String {
    rev(id, "Ann", "2026-01-01T00:00:00Z")
}

fn b(id: u32) -> String {
    rev(id, "Bob", "2026-01-02T00:00:00Z")
}

fn ins(attrs: &str, text: &str) -> String {
    format!("<w:ins {attrs}>{}</w:ins>", r(text))
}

fn del(attrs: &str, text: &str) -> String {
    format!(
        "<w:del {attrs}><w:r><w:delText xml:space=\"preserve\">{text}</w:delText></w:r></w:del>"
    )
}

/// The changes of a body.
fn of(body: &str) -> Vec<Change> {
    let s = state(body);
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    changes(&view.story(id).unwrap(), view.tree(id).unwrap(), None)
}

fn kinds(cs: &[Change]) -> Vec<ChangeKind> {
    cs.iter().map(|c| c.kind).collect()
}

#[test]
fn an_insertion_that_another_author_wrote_into_the_middle_of_is_still_one_change_of_its_author() {
    let body = p(&format!(
        "{}{}{}",
        ins(&a(1), "x"),
        ins(&b(2), "new"),
        ins(&a(9), "y")
    ));
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::Insertion, ChangeKind::Insertion]);
    assert_eq!(cs[0].ids, [1, 9]);
    assert_eq!(cs[0].author.as_deref(), Some("Ann"));
    assert_eq!(cs[1].ids, [2]);
}

#[test]
fn plain_text_between_two_insertions_of_one_author_makes_two_changes() {
    let body = p(&format!(
        "{}{}{}",
        ins(&a(1), "x"),
        r(" plain "),
        ins(&a(2), "y")
    ));
    let cs = of(&body);
    assert_eq!(cs.len(), 2);
    assert_eq!((cs[0].ids.clone(), cs[1].ids.clone()), (vec![1], vec![2]));
}

#[test]
fn a_deletion_by_another_author_inside_an_insertion_does_not_split_it() {
    let body = p(&format!(
        "<w:ins {}>{}<w:del {}><w:r><w:delText>mid</w:delText></w:r></w:del>{}</w:ins>",
        a(1),
        r("left"),
        b(2),
        r("right")
    ));
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::Insertion, ChangeKind::Deletion]);
    assert_eq!(cs[0].ids, [1]);
    assert_eq!(cs[1].ids, [2]);
}

#[test]
fn two_dates_of_one_author_are_two_changes() {
    let late = rev(2, "Ann", "2026-03-03T00:00:00Z");
    let body = p(&format!("{}{}", ins(&a(1), "x"), ins(&late, "y")));
    assert_eq!(of(&body).len(), 2);
}

#[test]
fn a_deletion_that_straddles_a_hyperlink_is_one_change() {
    let body = p(&format!(
        "{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>",
        del(&a(1), "one"),
        del(&a(2), "two")
    ));
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::Deletion]);
    assert_eq!(cs[0].ids, [1, 2]);
}

#[test]
fn a_paragraph_with_an_inserted_mark_and_only_inserted_content_is_one_paragraph_change() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        a(1),
        ins(&a(2), "new"),
        r("after")
    );
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::ParagraphInserted]);
    assert_eq!(cs[0].ids, [1, 2]);
    assert_eq!(cs[0].blocks, 0..1);
}

#[test]
fn a_deleted_paragraph_is_one_change_and_an_empty_one_with_a_deleted_mark_is_too() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr></w:p><w:p>{}</w:p>",
        a(1),
        del(&a(2), "gone"),
        a(3),
        r("after")
    );
    let cs = of(&body);
    assert_eq!(
        kinds(&cs),
        [ChangeKind::ParagraphDeleted, ChangeKind::ParagraphDeleted]
    );
    assert_eq!(cs[0].ids, [1, 2]);
    assert_eq!(cs[1].ids, [3]);
}

#[test]
fn a_mark_alone_with_plain_content_is_a_mark_change() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p><w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        a(1),
        r("first half"),
        r("second half"),
        b(2),
        r("joined"),
        r("tail")
    );
    let cs = of(&body);
    assert_eq!(
        kinds(&cs),
        [ChangeKind::MarkInserted, ChangeKind::MarkDeleted]
    );
}

#[test]
fn an_enter_and_the_text_typed_after_it_are_one_change_across_two_paragraphs() {
    // The Y shape: the first half has the inserted mark, the second starts with the inserted text.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}{}</w:p>",
        a(1),
        r("before"),
        ins(&a(2), "typed"),
        r(" after")
    );
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::ParagraphInserted]);
    assert_eq!(cs[0].ids, [1, 2]);
    assert_eq!(cs[0].blocks, 0..2);
}

#[test]
fn enter_then_text_then_enter_is_one_change() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}{}</w:p>",
        a(1),
        r("before"),
        a(3),
        ins(&a(2), "typed"),
        ins(&a(4), "more"),
        r(" after")
    );
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::ParagraphInserted]);
    // In document order: the two marks come before the text of their paragraphs.
    assert_eq!(cs[0].ids, [1, 3, 2, 4]);
    assert_eq!(cs[0].blocks, 0..3);
}

#[test]
fn an_inserted_row_takes_what_is_inserted_in_its_cells() {
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>\
         <w:trPr><w:ins {}/></w:trPr><w:tc><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p></w:tc></w:tr>\
         <w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl><w:p>{}</w:p>",
        a(1),
        a(2),
        ins(&a(3), "cell"),
        r("plain"),
        r("after")
    );
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::RowInserted]);
    assert_eq!(cs[0].ids, [1, 2, 3]);
    // The row is entry 1 and its paragraph entry 2.
    assert_eq!(cs[0].blocks, 1..3);
}

#[test]
fn a_deleted_row_is_one_change_and_foreign_revisions_in_it_stay_their_own() {
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>\
         <w:trPr><w:del {}/></w:trPr><w:tc><w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr>{}{}</w:p></w:tc></w:tr></w:tbl><w:p>{}</w:p>",
        a(1),
        a(2),
        del(&a(3), "gone"),
        ins(&b(4), "theirs"),
        r("after")
    );
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::RowDeleted, ChangeKind::Insertion]);
    assert_eq!(cs[0].ids, [1, 2, 3]);
    assert_eq!(cs[1].ids, [4]);
}

fn run_with_change(id: u32, snapshot: &str, text: &str) -> String {
    format!(
        "<w:r><w:rPr><w:b/><w:rPrChange {}><w:rPr>{snapshot}</w:rPr></w:rPrChange></w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r>",
        a(id)
    )
}

#[test]
fn consecutive_runs_with_one_old_formatting_are_one_change_whatever_their_ids() {
    let body = p(&format!(
        "{}{}{}",
        run_with_change(1, "<w:i/>", "a"),
        run_with_change(2, "<w:i/>", "b"),
        run_with_change(3, "", "c")
    ));
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::RunFormat, ChangeKind::RunFormat]);
    assert_eq!(cs[0].ids, [1, 2]);
    assert_eq!(cs[1].ids, [3]);
}

#[test]
fn plain_text_between_two_formatted_runs_separates_them() {
    let body = p(&format!(
        "{}{}{}",
        run_with_change(1, "", "a"),
        r("plain"),
        run_with_change(2, "", "b")
    ));
    assert_eq!(of(&body).len(), 2);
}

#[test]
fn consecutive_paragraphs_with_one_old_style_are_one_format_change() {
    let ppr = |id: u32, style: &str| {
        format!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/><w:pPrChange {}><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr></w:pPrChange></w:pPr>{}</w:p>",
            a(id),
            r("t")
        )
    };
    let body = format!("{}{}{}", ppr(1, "S"), ppr(2, "S"), ppr(3, "T"));
    let cs = of(&body);
    assert_eq!(
        kinds(&cs),
        [ChangeKind::ParagraphFormat, ChangeKind::ParagraphFormat]
    );
    assert_eq!(cs[0].ids, [1, 2]);
    assert_eq!(cs[0].blocks, 0..2);
    assert_eq!(cs[1].ids, [3]);
}

#[test]
fn the_two_sides_of_a_move_are_one_change_by_name() {
    let body = format!(
        "<w:p><w:moveFromRangeStart w:id=\"10\" w:name=\"move1\"/><w:moveFrom {}>{}</w:moveFrom><w:moveFromRangeEnd w:id=\"10\"/></w:p>\
         <w:p>{}<w:moveToRangeStart w:id=\"11\" w:name=\"move1\"/><w:moveTo {}>{}</w:moveTo><w:moveToRangeEnd w:id=\"11\"/></w:p>",
        a(1),
        r("moved"),
        r("elsewhere "),
        a(2),
        r("moved")
    );
    let cs = of(&body);
    assert_eq!(kinds(&cs), [ChangeKind::Move]);
    assert_eq!(cs[0].ids, [1, 2]);
    assert_eq!(cs[0].blocks, 0..2);
}

#[test]
fn two_moves_with_different_names_are_two_changes() {
    let mv = |name: &str, id: u32, text: &str| {
        format!(
            "<w:p><w:moveFromRangeStart w:id=\"{id}\" w:name=\"{name}\"/><w:moveFrom {}>{}</w:moveFrom><w:moveFromRangeEnd w:id=\"{id}\"/></w:p>",
            a(id + 100),
            r(text)
        )
    };
    let body = format!("{}{}", mv("m1", 1, "x"), mv("m2", 2, "y"));
    assert_eq!(of(&body).len(), 2);
}

#[test]
fn the_copy_of_a_change_in_the_untaken_branch_of_a_text_box_joins_the_taken_one() {
    let boxed = |ins_attrs: &str| format!("<w:p><w:ins {ins_attrs}>{}</w:ins></w:p>", r("boxed"));
    let body = format!(
        "<w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:txbxContent>{}</w:txbxContent></mc:Choice><mc:Fallback><w:txbxContent>{}</w:txbxContent></mc:Fallback></mc:AlternateContent></w:r></w:p>",
        boxed(&a(5)),
        boxed(&a(5))
    );
    let cs = of(&body);
    assert_eq!(cs.len(), 1, "{cs:?}");
    assert_eq!(cs[0].ids, [5]);
    assert_eq!(cs[0].nodes.len(), 2);
    assert_eq!(cs[0].branch, Branch::Taken);
}

#[test]
fn table_section_and_numbering_changes_are_changes_of_their_own() {
    let body = format!(
        "<w:tbl><w:tblPr><w:tblPrChange {}><w:tblPr/></w:tblPrChange></w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/><w:tblGridChange {}><w:tblGrid/></w:tblGridChange></w:tblGrid>\
         <w:tr><w:trPr><w:trPrChange {}><w:trPr/></w:trPrChange></w:trPr><w:tc><w:tcPr><w:tcPrChange {}><w:tcPr/></w:tcPrChange></w:tcPr><w:p>{}</w:p></w:tc></w:tr></w:tbl>\
         <w:p><w:pPr><w:sectPr><w:sectPrChange {}><w:sectPr/></w:sectPrChange></w:sectPr></w:pPr>{}</w:p>",
        a(1),
        a(2),
        a(3),
        a(4),
        r("cell"),
        a(5),
        r("end")
    );
    let cs = of(&body);
    assert_eq!(
        kinds(&cs),
        [
            ChangeKind::TableFormat,
            ChangeKind::GridChange,
            ChangeKind::RowFormat,
            ChangeKind::CellFormat,
            ChangeKind::SectionFormat
        ]
    );
}

#[test]
fn a_range_gives_the_changes_that_touch_it_and_a_change_that_straddles_the_edge_whole() {
    let body = format!(
        "<w:p>{}</w:p><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}{}</w:p><w:p>{}</w:p>",
        ins(&b(9), "other"),
        a(1),
        r("before"),
        ins(&a(2), "typed"),
        r(" after"),
        del(&b(10), "late")
    );
    let s = state(&body);
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let story = &story;
    // Entry 2 alone cuts the Enter pair, which is whole.
    let cs = changes(story, tree, Some(2..3));
    assert_eq!(cs.len(), 1);
    assert_eq!(
        (cs[0].ids.clone(), cs[0].blocks.clone()),
        (vec![1, 2], 1..3)
    );
    // Entry 0 and entry 3 each have one change of their own.
    assert_eq!(changes(story, tree, Some(0..1))[0].ids, [9]);
    assert_eq!(changes(story, tree, Some(3..4))[0].ids, [10]);
    // The whole story has the three, in document order.
    assert_eq!(changes(story, tree, None).len(), 3);
}

#[test]
fn every_revision_element_is_in_exactly_one_change() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}{}{}</w:p>",
        a(1),
        r("a"),
        ins(&a(2), "b"),
        del(&b(3), "c"),
        run_with_change(4, "", "d")
    );
    let s = state(&body);
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    let tree = view.tree(id).unwrap();
    let total = crate::wordml::part_revisions(tree).len();
    let cs = changes(&view.story(id).unwrap(), tree, None);
    let nodes: usize = cs.iter().map(|c| c.nodes.len()).sum();
    assert_eq!(nodes, total);
    let mut all: Vec<_> = cs.iter().flat_map(|c| c.nodes.iter().copied()).collect();
    all.sort_by_key(|n| format!("{n:?}"));
    all.dedup();
    assert_eq!(all.len(), total);
}
