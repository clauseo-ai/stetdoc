//! Tests of `Story::anchors`: pairing across paragraphs, markers between blocks, orphans, reused ids,
//! and what is not walked (text boxes, the branch Word does not read).

use super::testing::{document, part};
use super::*;
use std::sync::Arc;

fn anchors_of(body: &str) -> (Anchors, Story, crate::xml::SpanTree) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let anchors = story.anchors(&tree);
    (anchors, story, tree)
}

fn run(t: &str) -> String {
    format!("<w:r><w:t>{t}</w:t></w:r>")
}

#[test]
fn a_bookmark_that_spans_paragraphs_pairs_and_knows_both_paragraphs() {
    let (a, story, tree) = anchors_of(&format!(
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"intro\"/>{}</w:p><w:p>{}<w:bookmarkEnd w:id=\"1\"/></w:p>",
        run("a"),
        run("b")
    ));
    assert_eq!(a.bookmarks.len(), 1);
    let b = &a.bookmarks[0];
    assert_eq!((b.id, b.name.as_str(), b.hidden), (Some(1), "intro", false));
    let (start, end) = (b.start.unwrap(), b.end.unwrap());
    assert_eq!(start.paragraph, Some(story.blocks()[0].node));
    assert_eq!(end.paragraph, Some(story.blocks()[1].node));
    assert_eq!(tree.node_ref(start.node).local_name(), "bookmarkStart");
    assert_eq!(tree.node_ref(end.node).local_name(), "bookmarkEnd");
}

#[test]
fn an_id_reused_after_its_end_is_a_second_bookmark_and_orphans_are_kept() {
    let (a, _, _) = anchors_of(
        "<w:p><w:bookmarkEnd w:id=\"9\"/><w:bookmarkStart w:id=\"1\" w:name=\"one\"/><w:bookmarkEnd w:id=\"1\"/>\
         <w:bookmarkStart w:id=\"1\" w:name=\"two\"/></w:p><w:p><w:bookmarkStart w:id=\"7\" w:name=\"_GoBack\"/><w:bookmarkEnd w:id=\"7\"/></w:p>\
         <w:p><w:bookmarkStart w:name=\"noid\"/><w:bookmarkEnd/></w:p>",
    );
    let shape: Vec<(Option<i64>, &str, bool, bool, bool)> = a
        .bookmarks
        .iter()
        .map(|b| {
            (
                b.id,
                b.name.as_str(),
                b.start.is_some(),
                b.end.is_some(),
                b.hidden,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (Some(9), "", false, true, false),
            (Some(1), "one", true, true, false),
            (Some(1), "two", true, false, false),
            (Some(7), "_GoBack", true, true, true),
            (None, "noid", true, false, false),
            (None, "", false, true, false),
        ]
    );
}

#[test]
fn nested_bookmarks_with_the_same_id_pair_nearest_first() {
    let (a, story, tree) = anchors_of(
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"outer\"/><w:bookmarkStart w:id=\"1\" w:name=\"inner\"/>\
         <w:bookmarkEnd w:id=\"1\"/><w:bookmarkEnd w:id=\"1\"/></w:p>",
    );
    assert_eq!(a.bookmarks.len(), 2);
    let order: Vec<NodeId> = tree
        .node_ref(story.blocks()[0].node)
        .children()
        .map(|c| c.id())
        .collect();
    let at = |n: NodeId| order.iter().position(|&x| x == n).unwrap();
    let (outer, inner) = (&a.bookmarks[0], &a.bookmarks[1]);
    assert_eq!(
        (outer.name.as_str(), inner.name.as_str()),
        ("outer", "inner")
    );
    assert!(
        at(inner.end.unwrap().node) < at(outer.end.unwrap().node),
        "the first end closes the inner start"
    );
}

#[test]
fn markers_between_blocks_are_paired_with_no_paragraph() {
    let (a, story, _) = anchors_of(&format!(
        "<w:bookmarkStart w:id=\"1\" w:name=\"wide\"/><w:p>{}</w:p><w:tbl><w:tr><w:tc><w:bookmarkEnd w:id=\"1\"/>\
         <w:p>{}<w:commentRangeStart w:id=\"3\"/></w:p><w:commentRangeEnd w:id=\"3\"/></w:tc></w:tr></w:tbl>",
        run("a"),
        run("b")
    ));
    let b = &a.bookmarks[0];
    assert_eq!(b.start.unwrap().paragraph, None);
    assert_eq!(
        b.end.unwrap().paragraph,
        None,
        "a marker in a cell between paragraphs"
    );
    let c = &a.comments[0];
    assert_eq!(c.start.unwrap().paragraph, Some(story.blocks()[3].node));
    assert_eq!(c.end.unwrap().paragraph, None);
}

#[test]
fn a_comment_pairs_its_range_and_its_reference() {
    let (a, _, tree) = anchors_of(&format!(
        "<w:p><w:commentRangeStart w:id=\"4\"/>{}</w:p><w:p>{}<w:commentRangeEnd w:id=\"4\"/>\
         <w:r><w:commentReference w:id=\"4\"/></w:r><w:r><w:commentReference w:id=\"5\"/></w:r></w:p>",
        run("a"),
        run("b")
    ));
    assert_eq!(a.comments.len(), 2);
    let c = &a.comments[0];
    assert_eq!(c.id, 4);
    assert!(c.start.is_some() && c.end.is_some() && c.reference.is_some());
    assert_eq!(
        tree.node_ref(c.reference.unwrap().node).local_name(),
        "commentReference"
    );
    let only_reference = &a.comments[1];
    assert_eq!(only_reference.id, 5);
    assert!(only_reference.start.is_none() && only_reference.end.is_none());
    assert!(only_reference.reference.is_some());
}

#[test]
fn moves_pair_by_direction_and_id() {
    let (a, _, _) = anchors_of(
        "<w:p><w:moveFromRangeStart w:id=\"1\" w:name=\"move1\"/><w:moveToRangeStart w:id=\"1\" w:name=\"move2\"/></w:p>\
         <w:p><w:moveFromRangeEnd w:id=\"1\"/><w:moveToRangeEnd w:id=\"1\"/></w:p>",
    );
    assert_eq!(a.moves.len(), 2);
    let from = a.moves.iter().find(|m| m.from).unwrap();
    let to = a.moves.iter().find(|m| !m.from).unwrap();
    assert_eq!((from.name.as_str(), to.name.as_str()), ("move1", "move2"));
    assert!(from.start.is_some() && from.end.is_some() && to.start.is_some() && to.end.is_some());
}

#[test]
fn text_boxes_are_not_walked_and_neither_is_the_untaken_branch() {
    let (a, _, _) = anchors_of(&format!(
        "<w:p><w:bookmarkStart w:id=\"1\" w:name=\"host\"/><w:r><mc:AlternateContent>\
         <mc:Choice Requires=\"wps\"><w:drawing><w:txbxContent><w:p><w:bookmarkStart w:id=\"2\" w:name=\"in_box\"/></w:p></w:txbxContent></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><w:txbxContent><w:p><w:bookmarkStart w:id=\"2\" w:name=\"in_box_copy\"/></w:p></w:txbxContent></w:pict></mc:Fallback>\
         </mc:AlternateContent></w:r>{}<w:bookmarkEnd w:id=\"1\"/></w:p>\
         <mc:AlternateContent><mc:Choice Requires=\"w14\"><w:bookmarkStart w:id=\"5\" w:name=\"taken\"/></mc:Choice>\
         <mc:Fallback><w:bookmarkStart w:id=\"6\" w:name=\"untaken\"/></mc:Fallback></mc:AlternateContent>",
        run("x")
    ));
    let names: Vec<&str> = a.bookmarks.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["host", "taken"]);
    assert!(a.bookmarks[0].end.is_some());
}

#[test]
fn a_footnote_part_is_walked_from_its_root() {
    let tree = part(
        "word/footnotes.xml",
        "footnotes",
        "<w:footnote w:id=\"2\"><w:p><w:bookmarkStart w:id=\"1\" w:name=\"n\"/></w:p><w:p><w:bookmarkEnd w:id=\"1\"/></w:p></w:footnote>",
    );
    let story = Story::scan(
        &Arc::from("word/footnotes.xml"),
        StoryKind::Footnotes,
        &tree,
    )
    .unwrap();
    let a = story.anchors(&tree);
    assert_eq!(a.bookmarks.len(), 1);
    assert!(a.bookmarks[0].start.is_some() && a.bookmarks[0].end.is_some());
}

#[test]
fn a_story_with_no_markers_has_none_and_a_deep_tree_does_not_recurse() {
    let (a, _, _) = anchors_of("<w:p/>");
    assert_eq!(a, Anchors::default());
    let mut deep = String::from("<w:bookmarkStart w:id=\"1\" w:name=\"d\"/>");
    for _ in 0..200 {
        deep = format!("<w:smartTag>{deep}</w:smartTag>");
    }
    let (a, _, _) = anchors_of(&format!("<w:p>{deep}</w:p>"));
    assert_eq!(a.bookmarks.len(), 1);
}
