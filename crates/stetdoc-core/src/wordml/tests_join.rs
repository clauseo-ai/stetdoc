//! Tests of joined paragraphs: which entries read as one, and the map across them.

use super::testing::document;
use super::tests_text::{FINAL, ORIGINAL, REDLINE};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

fn p(mark: &str, t: &str) -> String {
    let ppr = if mark.is_empty() {
        String::new()
    } else {
        format!("<w:pPr><w:rPr><w:{mark} w:id=\"1\"/></w:rPr></w:pPr>")
    };
    format!("<w:p>{ppr}<w:r><w:t>{t}</w:t></w:r></w:p>")
}

fn scan(body: &str) -> (Story, SpanTree) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (story, tree)
}

#[test]
fn a_deleted_mark_joins_in_final_and_an_inserted_mark_in_original() {
    let (story, tree) = scan(&format!(
        "{}{}{}{}",
        p("del", "a"),
        p("del", "b"),
        p("", "c"),
        p("ins", "d")
    ));
    assert_eq!(
        story.join_run(&tree, 0, FINAL),
        0..3,
        "three paragraphs read as one: the last mark is kept"
    );
    assert_eq!(story.join_run(&tree, 1, FINAL), 1..3);
    assert_eq!(story.join_run(&tree, 2, FINAL), 2..3);
    assert_eq!(story.join_run(&tree, 0, ORIGINAL), 0..1);
    assert_eq!(
        story.join_run(&tree, 3, ORIGINAL),
        3..4,
        "the last paragraph has nothing to join"
    );
    assert_eq!(
        story.join_run(&tree, 0, REDLINE),
        0..1,
        "Redline never joins"
    );
    let (story, tree) = scan(&format!("{}{}", p("ins", "a"), p("", "b")));
    assert_eq!(story.join_run(&tree, 0, ORIGINAL), 0..2);
    assert_eq!(story.join_run(&tree, 0, FINAL), 0..1);
}

#[test]
fn a_moved_mark_joins_like_a_deleted_or_inserted_one() {
    let (story, tree) = scan(&format!(
        "{}{}{}{}",
        p("moveFrom", "a"),
        p("", "b"),
        p("moveTo", "c"),
        p("", "d")
    ));
    assert_eq!(story.join_run(&tree, 0, FINAL), 0..2);
    assert_eq!(story.join_run(&tree, 2, FINAL), 2..3);
    assert_eq!(story.join_run(&tree, 2, ORIGINAL), 2..4);
    assert_eq!(story.join_run(&tree, 0, ORIGINAL), 0..1);
}

#[test]
fn a_join_stops_at_a_table_the_end_of_a_cell_and_the_end_of_a_story() {
    let (story, tree) = scan(&format!(
        "{}<w:tbl><w:tr><w:tc>{}</w:tc><w:tc>{}</w:tc></w:tr></w:tbl>{}{}",
        p("del", "before the table"),
        p("del", "last of cell one"),
        p("", "cell two"),
        p("", "after"),
        p("del", "very last")
    ));
    assert_eq!(
        story.join_run(&tree, 0, FINAL),
        0..1,
        "the next block is a table"
    );
    let cell_one = 3;
    assert_eq!(story.blocks()[cell_one].kind, BlockKind::Paragraph);
    assert_eq!(
        story.join_run(&tree, cell_one, FINAL),
        cell_one..cell_one + 1,
        "the next paragraph is in another cell"
    );
    let last = story.blocks().len() - 1;
    assert_eq!(
        story.join_run(&tree, last, FINAL),
        last..last + 1,
        "a mark at the end of the story is ignored"
    );
}

#[test]
fn a_join_reads_through_a_block_level_control_and_stops_at_a_note() {
    let (story, tree) = scan(&format!(
        "{}<w:sdt><w:sdtContent>{}</w:sdtContent></w:sdt>",
        p("del", "a"),
        p("", "b")
    ));
    assert_eq!(story.join_run(&tree, 0, FINAL), 0..2);
    let notes = super::testing::part(
        "word/footnotes.xml",
        "footnotes",
        &format!(
            "<w:footnote w:id=\"2\">{}</w:footnote><w:footnote w:id=\"3\">{}</w:footnote>",
            p("del", "n"),
            p("", "m")
        ),
    );
    let story = Story::scan(
        &Arc::from("word/footnotes.xml"),
        StoryKind::Footnotes,
        &notes,
    )
    .unwrap();
    assert_eq!(
        story.join_run(&notes, 0, FINAL),
        0..1,
        "a mark at the end of a note joins nothing"
    );
}

#[test]
fn an_entry_that_is_not_a_paragraph_or_does_not_exist_gives_an_empty_run() {
    let (story, tree) = scan("<w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>");
    assert_eq!(story.join_run(&tree, 0, FINAL), 0..0);
    assert_eq!(story.join_run(&tree, 99, FINAL), 99..99);
    assert!(story.joined(&tree, 0, FINAL).is_none());
}

#[test]
fn joined_text_has_no_separator_and_a_range_gives_one_located_for_each_paragraph() {
    let (story, tree) = scan(&format!(
        "{}{}{}",
        p("del", "Hello "),
        p("del", "wor"),
        p("", "ld!")
    ));
    let (run, j) = story.joined(&tree, 0, FINAL).unwrap();
    assert_eq!(run, 0..3);
    assert_eq!(j.text(), "Hello world!");
    assert_eq!((j.len(), j.is_empty(), j.maps().len()), (12, false, 3));
    let at = j.locate(4..10).unwrap();
    assert_eq!(at.len(), 3);
    assert_eq!(
        (at[0].0, at[0].1.spans[0].start, at[0].1.spans[0].end),
        (0, 4, 6)
    );
    assert_eq!(
        (
            at[1].0,
            at[1].1.spans[0].start,
            at[1].1.spans[0].end,
            at[1].1.spans[0].whole
        ),
        (1, 0, 3, true)
    );
    assert_eq!(
        (at[2].0, at[2].1.spans[0].start, at[2].1.spans[0].end),
        (2, 0, 1)
    );
    let inside = j.locate(7..8).unwrap();
    assert_eq!(inside.len(), 1);
    assert_eq!(inside[0].0, 1);
    assert!(j.locate(5..13).is_none());
    let (a, b) = (8usize, 3usize);
    assert!(j.locate(a..b).is_none());
}

#[test]
fn a_gap_on_a_join_belongs_to_the_paragraph_on_its_left() {
    let (story, tree) = scan(&format!("{}{}", p("del", "ab"), p("", "cd")));
    let (_, j) = story.joined(&tree, 0, FINAL).unwrap();
    let (k, g) = j.gap(2).unwrap();
    assert_eq!(k, 0);
    assert_eq!(
        (g.left.is_some(), g.right.is_some()),
        (true, false),
        "the end of the first paragraph"
    );
    let (k, g) = j.gap(3).unwrap();
    assert_eq!(k, 1);
    assert_eq!((g.left, g.right), (Some(0), Some(0)));
    let (k, _) = j.gap(0).unwrap();
    assert_eq!(k, 0);
    let (k, g) = j.gap(4).unwrap();
    assert_eq!((k, g.right), (1, None));
    assert!(j.gap(5).is_none());
    let empty_range = j.locate(2..2).unwrap();
    assert_eq!((empty_range.len(), empty_range[0].0), (1, 0));
}

#[test]
fn empty_paragraphs_in_a_run_and_an_empty_joined_text_work() {
    let (story, tree) = scan(&format!(
        "{}{}{}",
        p("del", "a"),
        "<w:p><w:pPr><w:rPr><w:del w:id=\"2\"/></w:rPr></w:pPr></w:p>",
        p("", "b")
    ));
    let (run, j) = story.joined(&tree, 0, FINAL).unwrap();
    assert_eq!(run, 0..3);
    assert_eq!(j.text(), "ab");
    let (k, _) = j.gap(1).unwrap();
    assert_eq!(
        k, 0,
        "offset 1 is the end of the first paragraph even with an empty one after it"
    );
    let none = Joined::new(Vec::new(), FINAL);
    assert!(none.is_empty() && none.gap(0).is_none());
    assert!(
        none.locate(0..0).is_none(),
        "there is no map to hold an offset"
    );
}
