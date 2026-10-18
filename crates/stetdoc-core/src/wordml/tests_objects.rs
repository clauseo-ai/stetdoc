//! Tests of `Story::text_boxes`: which boxes are listed, which paragraph hosts each, which branch of an
//! `mc:AlternateContent` it is in, nesting, and the Word 2006 and Strict spellings. Each test says what
//! it proves.

use super::names::{W_S, W2006};
use super::testing::{document, parse, part};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

fn scan(body: &str) -> (SpanTree, Story) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (tree, story)
}

/// A paragraph whose one run holds a text box with `inner` as its content.
fn boxed(inner: &str) -> String {
    format!("<w:p><w:r><w:pict><w:txbxContent>{inner}</w:txbxContent></w:pict></w:r></w:p>")
}

fn texts(tree: &SpanTree, b: TextBox) -> Vec<String> {
    b.blocks(tree)
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.text(Mode::Final)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_text_box_is_listed_with_the_paragraph_that_holds_it() {
    // Proves: each `w:txbxContent` below a paragraph is a `TextBox` whose host is the entry index of
    // that paragraph, in document order, outside any `mc:AlternateContent`, and a paragraph with none
    // contributes none. The host index counts the entries of tables.
    let (tree, story) = scan(&format!(
        "<w:p/>{}<w:tbl><w:tr><w:tc><w:p/>{}</w:tc></w:tr></w:tbl>",
        boxed("<w:p><w:r><w:t>one</w:t></w:r></w:p>"),
        boxed("<w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>"),
    ));
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 2);
    assert_eq!(boxes[0].host, 1);
    assert_eq!(boxes[1].host, 5);
    assert!(boxes.iter().all(|b| b.branch == Branch::Outside));
    assert_eq!(texts(&tree, boxes[0]), ["one"]);
    assert_eq!(texts(&tree, boxes[1]), ["two", "three"]);
    for b in &boxes {
        assert_eq!(story.blocks()[b.host].kind, BlockKind::Paragraph);
        assert_eq!(tree.node_ref(b.content).local_name(), "txbxContent");
    }
}

#[test]
fn both_branches_of_an_alternate_content_are_listed_and_marked() {
    // Proves: a text box in the taken `mc:Choice` is `Taken` and the copy in the `mc:Fallback` is
    // `Untaken`, both with the same host, so a caller that shows text reads the first and one that
    // changes every copy reads both.
    let (tree, story) = scan(
        "<w:p><w:r><mc:AlternateContent>\
         <mc:Choice Requires=\"wps\"><w:drawing><wps:wsp><wps:txbx><w:txbxContent><w:p><w:r><w:t>same</w:t></w:r></w:p></w:txbxContent></wps:txbx></wps:wsp></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><v:shape xmlns:v=\"urn:schemas-microsoft-com:vml\"><v:textbox><w:txbxContent><w:p><w:r><w:t>same</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></mc:Fallback>\
         </mc:AlternateContent></w:r></w:p>",
    );
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 2);
    assert_eq!(boxes[0].branch, Branch::Taken);
    assert_eq!(boxes[1].branch, Branch::Untaken);
    assert_eq!((boxes[0].host, boxes[1].host), (0, 0));
    assert_eq!(texts(&tree, boxes[0]), texts(&tree, boxes[1]));
}

#[test]
fn the_fallback_is_taken_when_the_choice_requires_what_word_does_not_know() {
    // Proves: the branch marks follow the same rule as `revisions`: a `mc:Choice` whose `Requires`
    // prefix names an unknown namespace is not taken, so its box is `Untaken` and the Fallback's is
    // `Taken`.
    let (tree, story) = scan(
        "<w:p><w:r><mc:AlternateContent xmlns:zz=\"urn:not-understood\">\
         <mc:Choice Requires=\"zz\"><w:txbxContent><w:p/></w:txbxContent></mc:Choice>\
         <mc:Fallback><w:txbxContent><w:p/></w:txbxContent></mc:Fallback>\
         </mc:AlternateContent></w:r></w:p>",
    );
    let branches: Vec<_> = story.text_boxes(&tree).iter().map(|b| b.branch).collect();
    assert_eq!(branches, [Branch::Untaken, Branch::Taken]);
}

#[test]
fn the_marks_agree_with_the_revisions_of_the_same_story() {
    // Proves: `text_boxes` and `revisions` give the same branch to things in the same place: a
    // tracked insertion in each box has the branch of its box, in the same order.
    let content = "<w:txbxContent><w:p><w:ins w:id=\"1\" w:author=\"a\"><w:r><w:t>x</w:t></w:r></w:ins></w:p></w:txbxContent>";
    let (tree, story) = scan(&format!(
        "<w:p><w:r><mc:AlternateContent><mc:Choice Requires=\"wps\">{content}</mc:Choice><mc:Fallback>{content}</mc:Fallback></mc:AlternateContent></w:r></w:p>{}",
        boxed("<w:p><w:ins w:id=\"2\" w:author=\"a\"><w:r><w:t>y</w:t></w:r></w:ins></w:p>")
    ));
    let boxes = story.text_boxes(&tree);
    let revs = story.revisions(&tree);
    assert_eq!(boxes.len(), 3);
    assert_eq!(
        boxes.iter().map(|b| b.branch).collect::<Vec<_>>(),
        revs.iter().map(|r| r.branch).collect::<Vec<_>>()
    );
}

#[test]
fn a_text_box_inside_a_text_box_has_the_outermost_paragraph_as_host() {
    // Proves: boxes inside boxes are listed too, outer first and then the inner one, both hosted by
    // the paragraph of the outermost box (the blocks inside a box have no entries), and the inner
    // box is inside the outer one's content.
    let inner = boxed("<w:p><w:r><w:t>inner</w:t></w:r></w:p>");
    let (tree, story) = scan(&format!(
        "<w:p/>{}{}",
        boxed(&format!("<w:p><w:r><w:t>outer</w:t></w:r></w:p>{inner}")),
        boxed("<w:p/>"),
    ));
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 3);
    assert_eq!(boxes.iter().map(|b| b.host).collect::<Vec<_>>(), [1, 1, 2]);
    assert_eq!(texts(&tree, boxes[0])[0], "outer");
    assert_eq!(texts(&tree, boxes[1]), ["inner"]);
    let mut up = tree.node_ref(boxes[1].content).parent();
    let mut inside = false;
    while let Some(n) = up {
        inside |= n.id() == boxes[0].content;
        up = n.parent();
    }
    assert!(inside);
}

#[test]
fn nested_boxes_take_the_branch_of_every_ancestor() {
    // Proves: a box inside the untaken branch of an `mc:AlternateContent` that is itself inside a
    // taken box is `Untaken`, one in the taken branch of both is `Taken`, and everything inside an
    // untaken box is `Untaken` whatever its own branch.
    let ac = |inner: &str| {
        format!(
            "<mc:AlternateContent><mc:Choice Requires=\"wps\"><w:txbxContent>{inner}</w:txbxContent></mc:Choice>\
             <mc:Fallback><w:txbxContent>{inner}</w:txbxContent></mc:Fallback></mc:AlternateContent>"
        )
    };
    let nested = ac(&format!("<w:p><w:r>{}</w:r></w:p>", ac("<w:p/>")));
    let (tree, story) = scan(&format!("<w:p><w:r>{nested}</w:r></w:p>"));
    let branches: Vec<_> = story.text_boxes(&tree).iter().map(|b| b.branch).collect();
    // Document order: the outer choice, the choice and the fallback inside it, then the outer
    // fallback with its own two copies inside, untaken because their ancestor is.
    assert_eq!(
        branches,
        [
            Branch::Taken,
            Branch::Taken,
            Branch::Untaken,
            Branch::Untaken,
            Branch::Untaken,
            Branch::Untaken
        ]
    );
}

#[test]
fn the_word_2006_spelling_is_a_text_box_too() {
    // Proves: `txbxContent` in the Word 2006 namespace (as two Strict documents of the corpus write
    // it) is listed like `w:txbxContent`, and so is the Strict `w` spelling.
    let (tree, story) = scan(&format!(
        "<w:p><w:r><w:pict><w6:txbxContent xmlns:w6=\"{W2006}\"><w:p/></w6:txbxContent></w:pict></w:r></w:p>"
    ));
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 1);
    assert_eq!(tree.node_ref(boxes[0].content).namespace(), W2006);
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\"><w:body><w:p><w:r><w:pict><w:txbxContent><w:p><w:r><w:t>s</w:t></w:r></w:p></w:txbxContent></w:pict></w:r></w:p></w:body></w:document>"
        ),
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 1);
    assert_eq!(texts(&tree, boxes[0]), ["s"]);
}

#[test]
fn only_a_box_below_a_paragraph_is_listed() {
    // Proves: a `w:txbxContent` that is not below an entry paragraph (one standing where a block
    // stands, which the scan lists as opaque, or one in a block-level branch that is not taken) is not
    // a text box of the story.
    let (tree, story) = scan(
        "<w:txbxContent><w:p/></w:txbxContent>\
         <mc:AlternateContent><mc:Choice Requires=\"wps\"><w:p/></mc:Choice>\
         <mc:Fallback><w:p><w:r><w:txbxContent><w:p/></w:txbxContent></w:r></w:p></mc:Fallback></mc:AlternateContent>",
    );
    assert_eq!(story.blocks().len(), 1);
    assert!(story.text_boxes(&tree).is_empty());
}

#[test]
fn a_header_has_text_boxes_and_a_story_with_none_has_an_empty_list() {
    // Proves: `text_boxes` is not limited to the body: a header's paragraph hosts a box, with the
    // host an index into the header's own entries; a story with no box has an empty list.
    let tree = part(
        "word/header1.xml",
        "hdr",
        &format!("<w:p/>{}", boxed("<w:p><w:r><w:t>h</w:t></w:r></w:p>")),
    );
    let story = Story::scan(&Arc::from("word/header1.xml"), StoryKind::Header, &tree).unwrap();
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 1);
    assert_eq!(boxes[0].host, 1);
    let (tree, story) = scan("<w:p><w:r><w:t>none</w:t></w:r></w:p>");
    assert!(story.text_boxes(&tree).is_empty());
}

#[test]
fn a_table_inside_a_text_box_is_read_with_the_block_views() {
    // Proves: the blocks of a box are `Block` views on its content node, so a table in a box gives
    // its rows and cells like any other.
    let (tree, story) = scan(&boxed(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    ));
    let boxes = story.text_boxes(&tree);
    let blocks: Vec<_> = boxes[0].blocks(&tree).collect();
    assert_eq!(blocks.len(), 1);
    let Block::Table(t) = blocks[0] else {
        panic!("not a table");
    };
    let cell = t.rows().next().unwrap().cells().next().unwrap();
    let Some(Block::Paragraph(p)) = cell.blocks().next() else {
        panic!("no paragraph");
    };
    assert_eq!(p.text(Mode::Final), "cell");
}

#[test]
fn deeply_nested_boxes_are_walked_without_recursion() {
    // Proves: 40 boxes nested in each other (about 200 elements deep, under the parser's limit) are
    // all found with an explicit stack, each hosted by the one paragraph.
    let mut inner = String::from("<w:p/>");
    for _ in 0..40 {
        inner = boxed(&inner);
    }
    let (tree, story) = scan(&format!("<w:p/>{inner}"));
    let boxes = story.text_boxes(&tree);
    assert_eq!(boxes.len(), 40);
    assert!(boxes.iter().all(|b| b.host == 1));
}
