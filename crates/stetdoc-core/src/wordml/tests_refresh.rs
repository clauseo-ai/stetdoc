//! Tests of `Story::refresh`: it must give what a scan from scratch gives, the field index included,
//! after every kind of edit, and it must find new, removed and moved blocks itself. The shapes the
//! delta review found are fixed cases: a split inside a field result, an untracked delete of the
//! paragraph that holds a field's end or separate. A seeded random sequence runs the comparison after
//! every step, chaining each refresh from the one before as `state` does.

use super::names::W_T as W_NS;
use super::testing::document;
use super::tests_fields::{begin, end, instr, separate, text};
use super::tests_text::FINAL;
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

pub(super) struct Doc {
    pub(super) tree: SpanTree,
    pub(super) story: Story,
    pub(super) name: Arc<str>,
}

pub(super) fn kids(tree: &SpanTree, n: NodeId) -> Vec<NodeId> {
    tree.node_ref(n).children().map(|c| c.id()).collect()
}

/// The first element below `n` with this local name.
pub(super) fn find(tree: &SpanTree, n: NodeId, local: &str) -> Option<NodeId> {
    let mut stack = vec![tree.node_ref(n).children()];
    while let Some(top) = stack.last_mut() {
        let Some(c) = top.next() else {
            stack.pop();
            continue;
        };
        if c.is_element() && c.local_name() == local {
            return Some(c.id());
        }
        stack.push(c.children());
    }
    None
}

impl Doc {
    pub(super) fn new(body: &str) -> Doc {
        let tree = document(body);
        let name: Arc<str> = Arc::from("word/document.xml");
        let story = Story::scan(&name, StoryKind::Body, &tree).unwrap();
        Doc { tree, story, name }
    }

    pub(super) fn body(&self) -> NodeId {
        self.tree.root().first_child().unwrap().id()
    }

    /// The nodes of the blocks, in order.
    pub(super) fn blocks(&self) -> Vec<NodeId> {
        self.story.blocks().iter().map(|e| e.node).collect()
    }

    /// Runs the field pass on the current story, so the next refresh has an index to carry.
    pub(super) fn prime(&self) {
        let _ = self.story.fields(&self.tree);
    }

    /// Refreshes, checks the result against a scan from scratch, and chains it. Returns what the
    /// refresh reported and whether the field index was carried instead of left to be rebuilt.
    pub(super) fn refresh(&mut self, touched: &[NodeId]) -> (Refreshed, bool) {
        let r = Story::refresh(&self.story, &self.tree, touched).unwrap();
        let carried = r.fields_carried;
        // the chain continues from the refreshed value as it is, as `state` does
        let chained = r.story.clone();
        let scratch = Story::scan(&self.name, StoryKind::Body, &self.tree).unwrap();
        assert!(r.story.same_as(&scratch), "the blocks differ from a scan");
        assert_eq!(
            r.story.field_index(&self.tree),
            scratch.field_index(&self.tree),
            "the field index differs from a scan (carried: {carried})"
        );
        for i in 0..scratch.blocks().len() {
            let a = r.story.paragraph(&self.tree, i).map(|p| p.map());
            let b = scratch.paragraph(&self.tree, i).map(|p| p.map());
            match (a, b) {
                (Some(a), Some(b)) => {
                    assert_eq!(
                        a.text(super::tests_text::REDLINE),
                        b.text(super::tests_text::REDLINE)
                    );
                    assert_eq!(a.fields(), b.fields());
                    assert_eq!(a.items().len(), b.items().len());
                    assert_eq!(
                        a.items().iter().map(|x| x.flags).collect::<Vec<_>>(),
                        b.items().iter().map(|x| x.flags).collect::<Vec<_>>()
                    );
                }
                (None, None) => {}
                _ => panic!("entry {i} is a paragraph in only one"),
            }
        }
        self.story = chained;
        (r, carried)
    }

    pub(super) fn new_paragraph(&mut self, content: &[&str]) -> NodeId {
        let mut ed = self.tree.edit();
        let p = ed.create_element(W_NS, "p").unwrap();
        for c in content {
            let r = ed.create_element(W_NS, "r").unwrap();
            let t = ed.create_element(W_NS, "t").unwrap();
            let tx = ed.create_text(c).unwrap();
            ed.append_child(t, tx).unwrap();
            ed.append_child(r, t).unwrap();
            ed.append_child(p, r).unwrap();
        }
        p
    }

    pub(super) fn insert_at(&mut self, parent: NodeId, index: usize, node: NodeId) {
        let before = kids(&self.tree, parent).get(index).copied();
        self.tree
            .edit()
            .insert_before(parent, before, node)
            .unwrap();
    }

    pub(super) fn append_to_body(&mut self, node: NodeId) {
        let body = self.body();
        self.tree.edit().append_child(body, node).unwrap();
    }

    pub(super) fn detach(&mut self, node: NodeId) {
        self.tree.edit().detach(node).unwrap();
    }

    pub(super) fn set_text_of(&mut self, p: NodeId, to: &str) {
        let t = find(&self.tree, p, "t").unwrap();
        let tx = kids(&self.tree, t)[0];
        self.tree.edit().set_text(tx, to).unwrap();
    }

    pub(super) fn field_run(&mut self, p: NodeId, kind: &str) -> NodeId {
        let mut ed = self.tree.edit();
        let r = ed.create_element(W_NS, "r").unwrap();
        let f = ed.create_element(W_NS, "fldChar").unwrap();
        ed.set_attr(f, W_NS, "fldCharType", kind).unwrap();
        ed.append_child(r, f).unwrap();
        ed.append_child(p, r).unwrap();
        r
    }
}

/// Five paragraphs: a field that crosses three of them (a table of contents), and one outside.
pub(super) fn toc() -> Doc {
    Doc::new(&format!(
        "<w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>",
        text("before"),
        begin() + &instr(" TOC ") + &separate() + &text("one"),
        text("two"),
        text("three") + &end(),
        text("after")
    ))
}

fn status(doc: &Doc, i: usize) -> u8 {
    let p = doc.story.paragraph(&doc.tree, i).unwrap().map();
    p.items()
        .iter()
        .find(|x| x.kind == ItemKind::Text)
        .map_or(0, |x| x.flags & FIELD_RESULT)
}

// ---- the carried field index --------------------------------------------------------------------

#[test]
fn an_edit_of_text_inside_a_block_carries_the_index() {
    let mut doc = toc();
    doc.prime();
    let p = doc.blocks()[2];
    doc.set_text_of(p, "TWO");
    let (r, carried) = doc.refresh(&[p]);
    assert!(
        carried,
        "a text edit of a paragraph with no field character keeps the index"
    );
    assert!(r.added.is_empty() && r.removed.is_empty() && r.moved.is_empty());
    assert!(r.field_starts_changed.is_empty());
    assert_eq!(
        doc.story.paragraph(&doc.tree, 2).unwrap().text(FINAL),
        "TWO"
    );
}

#[test]
fn nothing_is_carried_when_the_previous_story_never_ran_the_pass() {
    let mut doc = toc();
    let p = doc.blocks()[0];
    doc.set_text_of(p, "x");
    let (_, carried) = doc.refresh(&[p]);
    assert!(
        !carried,
        "there was nothing to carry, and the pass runs on the first read"
    );
}

#[test]
fn a_split_inside_a_field_result_is_found_without_being_told() {
    // the new paragraph holds the field's end, and nothing says so in `touched`
    for touched_original in [true, false] {
        let mut doc = toc();
        doc.prime();
        let blocks = doc.blocks();
        let p3 = blocks[3];
        let runs = kids(&doc.tree, p3);
        let end_run = *runs.last().unwrap();
        doc.detach(end_run);
        let new = doc.new_paragraph(&[]);
        doc.tree.edit().append_child(new, end_run).unwrap();
        doc.insert_at(doc.body(), 4, new);
        let touched: Vec<NodeId> = if touched_original {
            vec![p3]
        } else {
            Vec::new()
        };
        let (r, carried) = doc.refresh(&touched);
        assert!(!carried);
        assert_eq!(r.added, [new]);
        assert!(r.removed.is_empty() && r.moved.is_empty());
        assert_eq!(doc.story.fields(&doc.tree)[0].paragraph_end, Some(new));
    }
}

#[test]
fn an_untracked_delete_of_the_paragraph_holding_a_fields_end_is_found() {
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    doc.detach(blocks[3]);
    let (r, carried) = doc.refresh(&[]);
    assert!(!carried);
    assert_eq!(r.removed, [blocks[3]]);
    assert_eq!(
        doc.story.fields(&doc.tree)[0].end,
        None,
        "the field has lost its end"
    );
    assert_eq!(
        r.field_starts_changed,
        [blocks[4]],
        "the paragraph after it now starts inside the field, and its text changed with no change to its XML"
    );
    assert_eq!(status(&doc, 3), FIELD_RESULT);
    assert_eq!(doc.story.field_anomalies(&doc.tree).len(), 1);
}

#[test]
fn an_untracked_delete_of_the_paragraph_holding_a_nested_field_with_its_separate_is_found() {
    // field code cannot cross a paragraph, so a separate is always in the paragraph of its begin
    let mut doc = Doc::new(&format!(
        "<w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>",
        begin() + &instr(" TOC ") + &separate() + &text("one"),
        begin() + &instr(" REF a ") + &separate() + &text("inner") + &end(),
        text("two") + &end()
    ));
    doc.prime();
    assert_eq!(doc.story.fields(&doc.tree).len(), 2);
    let blocks = doc.blocks();
    doc.detach(blocks[1]);
    let (r, carried) = doc.refresh(&[]);
    assert!(!carried);
    assert_eq!(r.removed, [blocks[1]]);
    let fields = doc.story.fields(&doc.tree);
    assert_eq!(fields.len(), 1);
    assert!(fields[0].separate.is_some() && fields[0].end.is_some());
}

#[test]
fn removing_a_paragraph_in_the_middle_of_a_field_result_changes_no_start() {
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    doc.detach(blocks[2]);
    let (r, carried) = doc.refresh(&[]);
    assert!(
        !carried,
        "the removed paragraph had an entry in the index, so the index is built again"
    );
    assert_eq!(r.removed, [blocks[2]]);
    assert!(
        r.field_starts_changed.is_empty(),
        "no other paragraph starts in another state"
    );
}

#[test]
fn a_field_character_detached_from_a_block_nobody_listed_is_still_noticed() {
    let mut doc = toc();
    doc.prime();
    let p3 = doc.blocks()[3];
    let end_run = *kids(&doc.tree, p3).last().unwrap();
    doc.detach(end_run);
    let (_, carried) = doc.refresh(&[]);
    assert!(!carried, "the field's end is gone from the tree");
    assert_eq!(doc.story.fields(&doc.tree)[0].end, None);
}

#[test]
fn a_new_paragraph_inside_a_field_span_takes_its_state() {
    let mut doc = toc();
    doc.prime();
    let new = doc.new_paragraph(&["inserted"]);
    doc.insert_at(doc.body(), 2, new);
    let (r, carried) = doc.refresh(&[]);
    assert!(!carried, "a new paragraph is a change of shape");
    assert_eq!(r.added, [new]);
    assert_eq!(
        status(&doc, 2),
        FIELD_RESULT,
        "the new paragraph is inside the field result"
    );
    assert_eq!(status(&doc, 0), 0);
    assert_eq!(
        r.field_starts_changed,
        [new],
        "it starts in a state it had none of"
    );
}

#[test]
fn a_new_paragraph_outside_every_field_shares_the_index_and_one_with_a_begin_does_not() {
    let mut doc = toc();
    doc.prime();
    // before every field, and after the last one: no field is open where it starts
    for at_end in [false, true] {
        let new = doc.new_paragraph(&["plain"]);
        let at = if at_end { doc.blocks().len() } else { 0 };
        doc.insert_at(doc.body(), at, new);
        let (r, carried) = doc.refresh(&[]);
        assert!(
            carried,
            "a paragraph with no markup, outside every field, changes no state"
        );
        assert_eq!(r.added, [new]);
        assert!(r.field_starts_changed.is_empty());
        doc.prime();
    }
    assert_eq!(status(&doc, 0), 0);
    // several in a row, the first of them after the last field
    let end = doc.blocks().len();
    let run: Vec<NodeId> = (0..3).map(|_| doc.new_paragraph(&["run"])).collect();
    for (k, p) in run.iter().enumerate() {
        doc.insert_at(doc.body(), end + k, *p);
    }
    let (r, carried) = doc.refresh(&[]);
    assert!(carried);
    assert_eq!(r.added, run);
    // one that holds a field character makes the pass run again
    let with_field = doc.new_paragraph(&[]);
    doc.field_run(with_field, "begin");
    doc.insert_at(doc.body(), 1, with_field);
    doc.prime();
    let (_, carried) = doc.refresh(&[]);
    assert!(
        !carried,
        "a new paragraph with a field character makes the pass run again"
    );
}

#[test]
fn a_paragraph_moved_between_places_outside_every_field_shares_the_index() {
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    // the paragraph before the field goes to after it
    doc.detach(blocks[0]);
    doc.append_to_body(blocks[0]);
    let (r, carried) = doc.refresh(&[]);
    assert!(carried);
    assert_eq!(r.moved, [blocks[0]]);
    assert!(r.field_starts_changed.is_empty());
    // a paragraph from outside to inside the result is another matter
    doc.prime();
    let last = *doc.blocks().last().unwrap();
    doc.detach(last);
    doc.insert_at(doc.body(), 2, last);
    let (_, carried) = doc.refresh(&[]);
    assert!(!carried);
}

#[test]
fn a_paragraph_moved_out_of_and_into_a_field_span_changes_its_state_and_is_reported_moved() {
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    // out: the middle paragraph goes to the end
    doc.detach(blocks[2]);
    doc.append_to_body(blocks[2]);
    let (r, carried) = doc.refresh(&[]);
    assert_eq!(r.moved, [blocks[2]]);
    assert!(!carried, "a moved paragraph is a change of shape");
    assert_eq!(r.field_starts_changed, [blocks[2]]);
    assert_eq!(status(&doc, 4), 0, "it is outside the field now");
    // in: the last paragraph (the one that went out) goes back between the first two
    doc.prime();
    let last = *doc.blocks().last().unwrap();
    doc.detach(last);
    doc.insert_at(doc.body(), 2, last);
    let (r, carried) = doc.refresh(&[]);
    assert_eq!(r.moved, [last]);
    assert!(!carried);
    assert_eq!(r.field_starts_changed, [last]);
    assert_eq!(status(&doc, 2), FIELD_RESULT);
}

#[test]
fn moving_the_paragraph_that_holds_a_begin_or_an_end_runs_the_pass_again() {
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    doc.detach(blocks[1]);
    doc.append_to_body(blocks[1]);
    let (r, carried) = doc.refresh(&[]);
    assert!(!carried);
    assert!(r.moved.contains(&blocks[1]));
    assert!(
        !r.field_starts_changed.is_empty(),
        "the paragraphs that were in the field result are not any more"
    );
}

#[test]
fn a_touched_paragraph_that_gains_a_field_character_runs_the_pass_again() {
    let mut doc = toc();
    doc.prime();
    let p = doc.blocks()[0];
    doc.field_run(p, "begin");
    let (_, carried) = doc.refresh(&[p]);
    assert!(!carried);
}

#[test]
fn a_touched_paragraph_that_loses_its_field_begin_runs_the_pass_again() {
    let mut doc = toc();
    doc.prime();
    let p = doc.blocks()[1];
    let first = kids(&doc.tree, p)[0];
    doc.detach(first);
    let (r, carried) = doc.refresh(&[p]);
    assert!(!carried);
    assert!(
        r.field_starts_changed.contains(&doc.blocks()[2]),
        "the next paragraph's text is no longer a field result"
    );
}

// ---- what is reported ---------------------------------------------------------------------------

#[test]
fn a_block_that_changes_parent_is_moved_even_when_the_order_is_kept() {
    let mut doc = Doc::new(&format!(
        "<w:p>{}</w:p><w:tbl><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl>",
        text("outside"),
        text("inside")
    ));
    let blocks = doc.blocks();
    assert_eq!(blocks.len(), 4);
    let cell = find(&doc.tree, doc.body(), "tc").unwrap();
    doc.detach(blocks[0]);
    doc.tree
        .edit()
        .insert_before(cell, None, blocks[0])
        .unwrap();
    let (r, _) = doc.refresh(&[]);
    assert!(r.moved.contains(&blocks[0]), "{:?}", r.moved);
    assert!(r.added.is_empty() && r.removed.is_empty());
}

#[test]
fn a_reorder_reports_the_fewest_moved_blocks() {
    let mut doc = Doc::new(
        &(0..8)
            .map(|i| format!("<w:p>{}</w:p>", text(&i.to_string())))
            .collect::<String>(),
    );
    let blocks = doc.blocks();
    doc.detach(blocks[1]);
    doc.append_to_body(blocks[1]);
    let (r, _) = doc.refresh(&[]);
    assert_eq!(
        r.moved,
        [blocks[1]],
        "one block moved, not the six it passed"
    );
    doc.detach(blocks[7]);
    doc.insert_at(doc.body(), 0, blocks[7]);
    let (r, _) = doc.refresh(&[]);
    assert_eq!(r.moved, [blocks[7]]);
}

#[test]
fn removing_a_table_removes_every_block_in_it() {
    let mut doc = Doc::new(&format!(
        "<w:p>{}</w:p><w:tbl><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl><w:p/>",
        text("a"),
        text("b")
    ));
    let blocks = doc.blocks();
    let table = blocks[1];
    doc.detach(table);
    let (r, _) = doc.refresh(&[]);
    assert_eq!(r.removed, [blocks[1], blocks[2], blocks[3]]);
}

// ---- what a table or row owns -------------------------------------------------------------------

#[test]
fn a_table_owns_its_properties_and_its_row_list_and_a_row_its_cell_properties_and_cell_list() {
    let doc = Doc::new(
        "<w:tbl><w:tblPr><w:tblW w:w=\"0\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"1\"/></w:tblGrid>\
         <w:tr><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"1\"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>\
         <w:tc><w:p/></w:tc></w:tr>\
         <w:sdt><w:sdtContent><w:tr><w:tc><w:tcPr/><w:p/></w:tc></w:tr></w:sdtContent></w:sdt></w:tbl>\
         <w:p/>",
    );
    let blocks = doc.blocks();
    let tree = &doc.tree;
    let name = |n: &NodeId| tree.node_ref(*n).local_name().to_string();
    let table = doc.story.own_parts(tree, 0).unwrap();
    assert_eq!(
        table.nodes.iter().map(name).collect::<Vec<_>>(),
        ["tblPr", "tblGrid"]
    );
    assert_eq!(
        table.list,
        [blocks[1], blocks[4]],
        "rows are found through a control"
    );
    let row = doc.story.own_parts(tree, 1).unwrap();
    assert_eq!(
        row.nodes.iter().map(name).collect::<Vec<_>>(),
        ["trPr", "tcPr"]
    );
    assert_eq!(row.list.iter().map(name).collect::<Vec<_>>(), ["tc", "tc"]);
    let paragraph = doc
        .story
        .blocks()
        .iter()
        .position(|e| e.kind == BlockKind::Paragraph)
        .unwrap();
    assert!(doc.story.own_parts(tree, paragraph).is_none());
    assert!(doc.story.own_parts(tree, 99).is_none());
}
