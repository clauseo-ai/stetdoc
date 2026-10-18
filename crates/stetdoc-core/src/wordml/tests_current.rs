//! A story is valid for the tree it was scanned from, as that tree was at scan time: it records the
//! tree's edit count, and the readers that rest on what it kept refuse a tree that has changed since.

use super::testing::document;
use super::tests_text::FINAL;
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

const BODY: &str = "<w:p><w:r><w:t>one</w:t></w:r></w:p>\
    <w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r>\
    <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>2</w:t></w:r>\
    <w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>\
    <w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr><w:r><w:t>three</w:t></w:r></w:p>\
    <w:p><w:r><w:t>four</w:t></w:r></w:p>";

fn scan(tree: &SpanTree) -> Story {
    Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap()
}

/// The text node of the first paragraph, which an edit changes.
fn first_text(tree: &SpanTree, story: &Story) -> NodeId {
    let p = tree.node_ref(story.blocks()[0].node);
    p.first_child()
        .unwrap()
        .first_child()
        .unwrap()
        .first_child()
        .unwrap()
        .id()
}

#[test]
fn a_story_is_current_for_the_tree_it_was_scanned_from_until_the_tree_is_edited() {
    let mut tree = document(BODY);
    let story = scan(&tree);
    assert!(story.is_current(&tree));
    assert_eq!(story.paragraph(&tree, 0).unwrap().text(FINAL), "one");
    assert_eq!(story.fields(&tree).len(), 1);
    assert!(story.block(&tree, 0).is_some());
    assert!(story.joined(&tree, 2, FINAL).is_some());

    let text = first_text(&tree, &story);
    tree.edit().set_text(text, "uno").unwrap();
    assert!(!story.is_current(&tree));
    assert!(
        story.paragraph(&tree, 0).is_none(),
        "the index is of the old tree"
    );
    assert!(story.block(&tree, 0).is_none());
    assert!(story.block(&tree, 1).is_none());
    assert!(story.joined(&tree, 2, FINAL).is_none());
    assert!(story.fields(&tree).is_empty());
    assert!(story.field_anomalies(&tree).is_empty());
}

#[test]
fn a_story_whose_index_was_built_before_the_edit_does_not_hand_it_out_afterwards() {
    // Proves: asking for the fields before the edit fills the story's cache, and the cache does not
    // answer for the edited tree, where a field has gone.
    let mut tree = document(BODY);
    let story = scan(&tree);
    assert_eq!(story.fields(&tree).len(), 1);
    let begin = story.fields(&tree)[0].begin;
    let run = tree.node_ref(begin).parent().unwrap().id();
    tree.edit().detach(run).unwrap();
    assert!(story.fields(&tree).is_empty());
    let refreshed = Story::refresh(&story, &tree, &[story.blocks()[1].node]).unwrap();
    assert!(refreshed.story.is_current(&tree));
    assert_eq!(
        refreshed.story.paragraph(&tree, 1).unwrap().text(FINAL),
        "2"
    );
}

#[test]
fn a_refresh_gives_a_story_that_is_current_and_a_scan_does_too() {
    let mut tree = document(BODY);
    let story = scan(&tree);
    let text = first_text(&tree, &story);
    tree.edit().set_text(text, "uno").unwrap();
    let refreshed = Story::refresh(&story, &tree, &[story.blocks()[0].node]).unwrap();
    assert!(refreshed.story.is_current(&tree));
    assert_eq!(
        refreshed.story.paragraph(&tree, 0).unwrap().text(FINAL),
        "uno"
    );
    assert!(refreshed.story.same_as(&scan(&tree)));
    assert!(scan(&tree).is_current(&tree));
    assert!(!story.is_current(&tree), "the old story stays old");
}

#[test]
fn a_change_that_changes_nothing_leaves_the_story_current_and_a_clone_has_the_same_history() {
    let mut tree = document(BODY);
    let story = scan(&tree);
    let text = first_text(&tree, &story);
    // a detach of a node that is detached is not an edit
    let loose = tree.edit().create_text("x").unwrap();
    let before = tree.edit_count();
    tree.edit().detach(loose).unwrap();
    assert_eq!(tree.edit_count(), before);
    let story = scan(&tree);
    assert!(story.is_current(&tree));
    let copy = tree.clone();
    assert!(
        story.is_current(&copy),
        "a clone has the count of the tree it came from"
    );
    tree.edit().set_text(text, "uno").unwrap();
    assert!(
        story.is_current(&copy),
        "and the copy does not follow the original"
    );
    assert!(!story.is_current(&tree));
}
