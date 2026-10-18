//! `mc:ProcessContent` and `mc:Ignorable`: what Word reads under an element in an ignorable namespace
//! that the document names. One rule, `schema`'s, read through `mc.rs`.

use super::names::{MC, W_T as W_NS};
use super::testing::parse;
use super::tests_text::{FINAL, check_invariants};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

/// A document whose root makes `x` ignorable and names `x:wrap` for processing.
fn doc(body: &str) -> SpanTree {
    parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_NS}\" xmlns:mc=\"{MC}\" xmlns:x=\"urn:example:ext\" xmlns:y=\"urn:example:other\" \
             mc:Ignorable=\"x\" mc:ProcessContent=\"x:wrap\"><w:body>{body}</w:body></w:document>"
        ),
    )
}

fn story(tree: &SpanTree) -> Story {
    Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap()
}

#[test]
fn a_processed_element_in_a_paragraph_is_read_in_place_and_another_ignorable_one_is_skipped() {
    // Proves: `x:wrap` is replaced by its children, `x:note` is skipped with its content and without an
    // anomaly, and `y:foreign` (not ignorable) is skipped with its content and counted.
    let tree = doc(
        "<w:p><w:r><w:t>a</w:t></w:r><x:wrap><w:r><w:t>b</w:t></w:r></x:wrap>\
         <x:note><w:r><w:t>hidden</w:t></w:r></x:note><y:foreign><w:r><w:t>lost</w:t></w:r></y:foreign></w:p>",
    );
    let s = story(&tree);
    let m = s.paragraph(&tree, 0).unwrap().map();
    check_invariants(&m);
    assert_eq!(m.text(FINAL), "ab");
    let kinds: Vec<AnomalyKind> = m.anomalies().iter().map(|a| a.kind).collect();
    assert_eq!(kinds, [AnomalyKind::UnknownElement]);
}

#[test]
fn a_processed_element_in_a_run_and_in_a_nest_is_read() {
    let tree = doc(
        "<w:p><w:r><x:wrap><w:t>c</w:t><x:wrap><w:t>d</w:t></x:wrap></x:wrap><x:note><w:t>no</w:t></x:note></w:r></w:p>",
    );
    let s = story(&tree);
    let m = s.paragraph(&tree, 0).unwrap().map();
    assert_eq!(m.text(FINAL), "cd");
    assert!(m.anomalies().is_empty());
}

#[test]
fn blocks_in_a_processed_element_are_blocks_and_an_ignored_one_has_none() {
    let tree = doc(
        "<x:wrap><w:p><w:r><w:t>one</w:t></w:r></w:p></x:wrap><x:note><w:p/></x:note>\
         <w:tbl><x:wrap><w:tr><w:tc><x:wrap><w:p/></x:wrap></w:tc></w:tr></x:wrap></w:tbl>",
    );
    let s = story(&tree);
    let kinds: Vec<BlockKind> = s.blocks().iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            BlockKind::Paragraph,
            BlockKind::Table,
            BlockKind::Row,
            BlockKind::Paragraph
        ]
    );
    assert!(
        s.opaque().is_empty(),
        "an ignorable element is not opaque content"
    );
    assert!(s.anomalies().is_empty(), "{:?}", s.anomalies());
    assert_eq!(s.paragraph(&tree, 0).unwrap().text(FINAL), "one");
    // the views and what a table owns read the same rows
    let own = s.own_parts(&tree, 1).unwrap();
    assert_eq!(own.list.len(), 1);
    let Some(Block::Table(table)) = s.block(&tree, 1) else {
        panic!("entry 1 is a table");
    };
    assert_eq!(table.rows().count(), 1);
    let Some(Block::Row(row)) = s.block(&tree, 2) else {
        panic!("entry 2 is a row");
    };
    assert_eq!(row.cells().next().unwrap().blocks().count(), 1);
}

#[test]
fn an_element_that_is_not_ignorable_is_opaque_at_the_block_level() {
    let tree = doc("<y:foreign><w:p/></y:foreign><w:p/>");
    let s = story(&tree);
    assert_eq!(s.blocks().len(), 1);
    assert_eq!(s.opaque().len(), 1);
}

#[test]
fn anchors_are_found_in_a_processed_element_and_not_in_an_ignorable_one_that_is_skipped() {
    // Proves: the anchor walk reads the elements Word reads, by `mc::disposition`: a bookmark in
    // `x:wrap` is paired, one in `x:note` (ignorable and not named) is not there for Word, so it is
    // not found, and one in the processed element at the block level is found with no paragraph.
    let tree = doc(
        "<w:p><x:wrap><w:bookmarkStart w:id=\"1\" w:name=\"a\"/></x:wrap><w:r><w:t>t</w:t></w:r>\
         <x:note><w:bookmarkStart w:id=\"2\" w:name=\"skipped\"/></x:note>\
         <w:bookmarkEnd w:id=\"1\"/></w:p>\
         <x:wrap><w:bookmarkStart w:id=\"3\" w:name=\"between\"/></x:wrap>\
         <x:note><w:bookmarkEnd w:id=\"3\"/></x:note>",
    );
    let s = story(&tree);
    let anchors = s.anchors(&tree);
    let seen: Vec<(Option<i64>, bool, bool)> = anchors
        .bookmarks
        .iter()
        .map(|b| (b.id, b.start.is_some(), b.end.is_some()))
        .collect();
    assert_eq!(seen, [(Some(1), true, true), (Some(3), true, false)]);
    let first = &anchors.bookmarks[0];
    assert!(first.start.unwrap().paragraph.is_some());
    assert!(anchors.bookmarks[1].start.unwrap().paragraph.is_none());
}

#[test]
fn a_deleted_mark_joins_a_paragraph_that_is_in_a_processed_element() {
    // Proves: `container_of` reads through an element that `mc:ProcessContent` names, as the story scan
    // does, so a paragraph before the element and the one inside it are in the same container.
    let tree = doc(
        "<w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr><w:r><w:t>a</w:t></w:r></w:p>\
         <x:wrap><w:p><w:pPr><w:rPr><w:del w:id=\"2\"/></w:rPr></w:pPr><w:r><w:t>b</w:t></w:r></w:p>\
         <w:p><w:r><w:t>c</w:t></w:r></w:p></x:wrap>",
    );
    let s = story(&tree);
    assert_eq!(s.blocks().len(), 3);
    assert_eq!(s.join_run(&tree, 0, FINAL), 0..3);
    let (run, joined) = s.joined(&tree, 0, FINAL).unwrap();
    assert_eq!((run, joined.text()), (0..3, "abc"));
}

#[test]
fn a_deleted_mark_does_not_join_across_a_cell_even_through_a_processed_element() {
    // The cell is a container, and a processed element inside it is not another one.
    let tree = doc(
        "<w:tbl><w:tr><w:tc><x:wrap><w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr></w:p></x:wrap></w:tc>\
         <w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>",
    );
    let s = story(&tree);
    let first = (0..s.blocks().len())
        .find(|&i| s.blocks()[i].kind == BlockKind::Paragraph)
        .unwrap();
    assert_eq!(s.join_run(&tree, first, FINAL), first..first + 1);
}
