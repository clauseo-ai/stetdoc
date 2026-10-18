//! A window of entries gives what the whole story gives for it (law R10), on the shapes that make a
//! window hard: a change longer than the window, markers between blocks that pair with markers far away,
//! and wrappers around blocks.

use super::laws_select::{check_ids, check_windows};
use super::tests_support::*;

fn a(id: u32) -> String {
    format!("w:id=\"{id}\" w:author=\"Ann\" w:date=\"2026-01-01T00:00:00Z\"")
}

fn paragraphs(n: usize) -> Vec<String> {
    (0..n).map(|i| p(&r(&format!("Clause {i}")))).collect()
}

#[test]
fn a_change_longer_than_the_window_is_whole_in_a_window_of_one_block() {
    // Twelve consecutive paragraphs with the same pending change to their properties are one change.
    let body: String = (0..24)
        .map(|i| {
            if (4..16).contains(&i) {
                format!(
                    "<w:p><w:pPr><w:jc w:val=\"center\"/><w:pPrChange {}><w:pPr/></w:pPrChange></w:pPr>{}</w:p>",
                    a(100 + i),
                    r("x")
                )
            } else {
                p(&r("y"))
            }
        })
        .collect();
    let s = state(&body);
    check_windows(&s).unwrap();
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let one = super::changes::changes(&story, tree, Some(9..10));
    assert_eq!(one.len(), 1);
    assert_eq!(
        one[0].blocks,
        4..16,
        "the whole change, not the part in the window"
    );
    check_ids(&s).unwrap();
}

#[test]
fn custom_xml_markers_between_blocks_pair_with_the_marker_far_away() {
    let mut parts = paragraphs(20);
    parts.insert(2, format!("<w:customXmlInsRangeStart {}/>", a(900)));
    parts.insert(18, "<w:customXmlInsRangeEnd w:id=\"900\"/>".to_owned());
    let s = state(&parts.concat());
    check_windows(&s).unwrap();
    check_ids(&s).unwrap();
}

#[test]
fn a_wrapper_around_blocks_is_read_with_each_block_it_holds() {
    let mut parts = paragraphs(12);
    let wrapped = format!(
        "<w:ins {}>{}{}{}</w:ins>",
        a(7),
        p(&r("one")),
        p(&r("two")),
        p(&r("three"))
    );
    parts.insert(5, wrapped);
    let s = state(&parts.concat());
    check_windows(&s).unwrap();
    check_ids(&s).unwrap();
}

#[test]
fn a_wrapper_around_blocks_in_a_cell_is_read_with_the_row() {
    let cell = format!(
        "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}<w:ins {}>{}{}</w:ins>{}</w:tc>",
        p(&r("before")),
        a(8),
        p(&r("one")),
        p(&r("two")),
        p(&r("after"))
    );
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>{cell}</w:tr></w:tbl>"
    );
    let mut parts = paragraphs(10);
    parts.insert(5, table);
    let s = state(&parts.concat());
    check_windows(&s).unwrap();
    check_ids(&s).unwrap();
}

#[test]
fn a_move_touches_the_blocks_that_hold_its_two_ends_and_not_the_ones_between() {
    let mut parts = paragraphs(30);
    parts[3] = p(&format!(
        "<w:moveFromRangeStart w:id=\"10\" w:name=\"m\"/><w:moveFrom {}>{}</w:moveFrom><w:moveFromRangeEnd w:id=\"10\"/>",
        a(40),
        r("moved text")
    ));
    parts[25] = p(&format!(
        "<w:moveToRangeStart w:id=\"11\" w:name=\"m\"/><w:moveTo {}>{}</w:moveTo><w:moveToRangeEnd w:id=\"11\"/>",
        a(41),
        r("moved text")
    ));
    let s = state(&parts.concat());
    check_windows(&s).unwrap();
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let all = super::changes::changes(&story, tree, None);
    let moves: Vec<_> = all
        .iter()
        .filter(|c| c.kind == super::changes::ChangeKind::Move)
        .collect();
    assert_eq!(moves.len(), 1, "{all:?}");
    assert_eq!(moves[0].touches, [3, 25]);
    assert!(super::changes::changes(&story, tree, Some(10..12)).is_empty());
}

#[test]
fn an_enter_chain_that_the_window_cuts_in_two_is_one_change_in_a_window_inside_its_tail() {
    // Five paragraphs in a chain: an Enter, the text typed, the next Enter, and so on, by one author. A
    // window around its last paragraph that starts in the middle of the chain would see whole paragraphs
    // and a lone insertion, unless it grows to find where the chain begins.
    let mark = |id: u32| format!("<w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>", a(id));
    let typed = |id: u32, t: &str| format!("<w:ins {}>{}</w:ins>", a(id), r(t));
    let mut parts = paragraphs(10);
    parts.push(format!("<w:p>{}{}</w:p>", mark(1), r("one")));
    parts.push(format!("<w:p>{}{}</w:p>", mark(3), typed(2, "two")));
    parts.push(format!("<w:p>{}{}</w:p>", mark(5), typed(4, "three")));
    parts.push(format!("<w:p>{}{}</w:p>", mark(7), typed(6, "four")));
    parts.push(format!("<w:p>{}</w:p>", typed(8, "five")));
    parts.extend(paragraphs(10));
    let s = state(&parts.concat());
    check_windows(&s).unwrap();
    check_ids(&s).unwrap();
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    let tree = view.tree(id).unwrap();
    let tail = super::changes::changes(&story, tree, Some(14..15));
    assert_eq!(tail.len(), 1, "{tail:?}");
    assert_eq!(tail[0].touches, [10, 11, 12, 13, 14], "{tail:?}");
}
