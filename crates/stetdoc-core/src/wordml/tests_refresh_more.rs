//! More cases for `Story::refresh`: the shapes the code review found, and the wrapper changes that
//! `Refreshed::context_changed` reports.

use super::names::W_T as W_NS;
use super::tests_fields::{begin, end, instr, separate, text};
use super::tests_refresh::{Doc, find, kids, toc};
use super::tests_text::{FINAL, ORIGINAL};
use super::*;

fn element(doc: &mut Doc, name: &str) -> NodeId {
    doc.tree.edit().create_element(W_NS, name).unwrap()
}

fn append(doc: &mut Doc, parent: NodeId, child: NodeId) {
    doc.tree.edit().append_child(parent, child).unwrap();
}

// ---- removed paragraphs ---------------------------------------------------------------------------

#[test]
fn a_paragraph_moved_where_the_scan_does_not_read_is_removed_and_its_field_end_is_not_carried() {
    // The paragraph that holds the field's end goes into a content control that has no
    // `w:sdtContent`. It is still in the tree, so every part of the field is still attached, but the
    // scan no longer lists it, and the field has lost its end.
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    let sdt = element(&mut doc, "sdt");
    let body = doc.body();
    doc.tree
        .edit()
        .insert_before(body, Some(blocks[3]), sdt)
        .unwrap();
    doc.detach(blocks[3]);
    append(&mut doc, sdt, blocks[3]);
    let (r, carried) = doc.refresh(&[]);
    assert!(!carried);
    assert_eq!(r.removed, [blocks[3]]);
    assert_eq!(doc.story.fields(&doc.tree)[0].end, None);
    assert_eq!(r.field_starts_changed, [blocks[4]]);
}

#[test]
fn a_paragraph_moved_where_the_scan_does_not_read_and_not_in_a_field_is_removed_and_the_index_is_kept()
 {
    // A paragraph with no field state and no markup can leave the scan and the shared index stays right.
    let mut doc = toc();
    doc.prime();
    let blocks = doc.blocks();
    let sdt = element(&mut doc, "sdt");
    let body = doc.body();
    doc.tree.edit().insert_before(body, None, sdt).unwrap();
    doc.detach(blocks[4]);
    append(&mut doc, sdt, blocks[4]);
    let (r, carried) = doc.refresh(&[]);
    assert!(carried);
    assert_eq!(r.removed, [blocks[4]]);
}

#[test]
fn an_untracked_delete_of_a_paragraph_with_a_stray_instruction_in_a_result_is_found() {
    // The instruction text in a result is stray: it does not join the instruction, and the paragraph
    // that holds it still counts as one that held field markup.
    let mut doc = Doc::new(&format!(
        "<w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>",
        begin() + &instr(" REF a ") + &separate() + &text("one"),
        instr(" stray ") + &text("two"),
        text("three") + &end()
    ));
    doc.prime();
    assert_eq!(doc.story.fields(&doc.tree)[0].instr, " REF a ");
    let blocks = doc.blocks();
    doc.detach(blocks[1]);
    let (r, carried) = doc.refresh(&[]);
    assert!(!carried, "the removed paragraph held field markup");
    assert_eq!(r.removed, [blocks[1]]);
    assert_eq!(doc.story.fields(&doc.tree)[0].instr, " REF a ");
}

// ---- wrappers around a block ------------------------------------------------------------------------

fn texts_of(doc: &Doc, i: usize) -> (String, String) {
    let p = doc.story.paragraph(&doc.tree, i).unwrap();
    (p.text(FINAL), p.text(ORIGINAL))
}

#[test]
fn a_block_wrapped_into_a_block_level_insertion_and_out_again_is_reported() {
    let mut doc = Doc::new("<w:p><w:r><w:t>a</w:t></w:r></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p>");
    let blocks = doc.blocks();
    let ins = element(&mut doc, "ins");
    let body = doc.body();
    doc.tree
        .edit()
        .insert_before(body, Some(blocks[1]), ins)
        .unwrap();
    doc.detach(blocks[1]);
    append(&mut doc, ins, blocks[1]);
    let (r, _) = doc.refresh(&[]);
    assert_eq!(r.context_changed, [blocks[1]]);
    assert!(r.moved.is_empty() && r.added.is_empty());
    assert_eq!(texts_of(&doc, 1), ("b".into(), "".into()));
    // and out again: the paragraph goes back to the body, and the wrapper is left empty
    doc.detach(blocks[1]);
    doc.tree
        .edit()
        .insert_before(body, None, blocks[1])
        .unwrap();
    let (r, _) = doc.refresh(&[]);
    assert_eq!(r.context_changed, [blocks[1]]);
    assert_eq!(texts_of(&doc, 1), ("b".into(), "b".into()));
}

#[test]
fn a_row_or_a_cell_that_gains_a_revision_reports_the_paragraphs_in_it() {
    let mut doc = Doc::new(
        "<w:p/><w:tbl><w:tr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    let blocks = doc.blocks();
    // blocks: p, tbl, tr, p (x), p (y)
    let row = blocks[2];
    let tr_pr = element(&mut doc, "trPr");
    let marker = element(&mut doc, "del");
    append(&mut doc, tr_pr, marker);
    let first = kids(&doc.tree, row)[0];
    doc.tree
        .edit()
        .insert_before(row, Some(first), tr_pr)
        .unwrap();
    let (r, _) = doc.refresh(&[]);
    assert_eq!(
        r.context_changed,
        [blocks[3], blocks[4]],
        "the paragraphs of the row, and not the row, whose own revision is in its bytes"
    );
    assert_eq!(texts_of(&doc, 3), ("".into(), "x".into()));
    // one cell gains a cell insertion on top: its paragraph is in neither mode
    let cell = find(&doc.tree, row, "tc").unwrap();
    let tc_pr = element(&mut doc, "tcPr");
    let cell_ins = element(&mut doc, "cellIns");
    append(&mut doc, tc_pr, cell_ins);
    let first = kids(&doc.tree, cell)[0];
    doc.tree
        .edit()
        .insert_before(cell, Some(first), tc_pr)
        .unwrap();
    let (r, _) = doc.refresh(&[]);
    assert_eq!(r.context_changed, [blocks[3]]);
    assert_eq!(texts_of(&doc, 3), ("".into(), "".into()));
}

#[test]
fn an_edit_that_changes_no_wrapper_reports_none() {
    let mut doc = toc();
    doc.prime();
    let p = doc.blocks()[2];
    doc.set_text_of(p, "TWO");
    let (r, _) = doc.refresh(&[p]);
    assert!(r.context_changed.is_empty());
}

// ---- the first body ---------------------------------------------------------------------------------

#[test]
fn a_second_body_is_not_read_and_is_counted() {
    let tree = super::testing::parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_NS}\"><w:body><w:p><w:r><w:t>read</w:t></w:r></w:p></w:body>\
             <w:body><w:p><w:r><w:t>never seen</w:t></w:r></w:p></w:body><w:body/></w:document>"
        ),
    );
    let story = Story::scan(
        &std::sync::Arc::from("word/document.xml"),
        StoryKind::Body,
        &tree,
    )
    .unwrap();
    assert_eq!(story.blocks().len(), 1);
    let kinds: Vec<AnomalyKind> = story.anomalies().iter().map(|a| a.kind).collect();
    assert_eq!(kinds, [AnomalyKind::ExtraBody, AnomalyKind::ExtraBody]);
}

#[test]
fn a_table_reads_its_rows_through_the_branch_of_an_alternate_content() {
    // Proves: `own_parts` lists the rows of the branch Word reads, and the `mc:AlternateContent` is not
    // one of the nodes the table owns.
    let doc = Doc::new(
        "<w:tbl><w:tblPr/><w:tblGrid/><mc:AlternateContent><mc:Choice Requires=\"w14\"><w:tr><w:tc><w:p/></w:tc></w:tr></mc:Choice>\
         <mc:Fallback><w:tr><w:tc><w:p/></w:tc></w:tr></mc:Fallback></mc:AlternateContent></w:tbl>",
    );
    let blocks = doc.blocks();
    assert_eq!(
        blocks.len(),
        3,
        "a table, the row of the taken branch, its paragraph"
    );
    let own = doc.story.own_parts(&doc.tree, 0).unwrap();
    assert_eq!(own.list, [blocks[1]]);
    let names: Vec<String> = own
        .nodes
        .iter()
        .map(|n| doc.tree.node_ref(*n).local_name().to_string())
        .collect();
    assert_eq!(names, ["tblPr", "tblGrid"]);
}

// ---- a paragraph that goes to another note or comment ------------------------------------------------

#[test]
fn a_paragraph_that_moves_to_the_next_comment_is_moved_though_the_block_list_looks_the_same() {
    // Proves: the last paragraph of one comment, moved to the start of the next, keeps its place in the
    // list of blocks and its parent, and is still reported as moved, because the comment is part of
    // where it is. It begins a field that stays open to the end of its unit, so what the next
    // paragraph reads changes with it.
    let tree = super::testing::part(
        "word/comments.xml",
        "comments",
        &format!(
            "<w:comment w:id=\"1\"><w:p>{}</w:p><w:p>{}</w:p></w:comment>\
             <w:comment w:id=\"2\"><w:p>{}</w:p></w:comment>",
            text("a"),
            begin() + &instr(" REF x ") + &separate() + &text("open"),
            text("b")
        ),
    );
    let name: std::sync::Arc<str> = std::sync::Arc::from("word/comments.xml");
    let prev = Story::scan(&name, StoryKind::Comments, &tree).unwrap();
    let _ = prev.fields(&tree);
    let blocks: Vec<NodeId> = prev.blocks().iter().map(|e| e.node).collect();
    let mut tree = tree;
    let second_comment = tree
        .node_ref(tree.root().id())
        .child_elements()
        .nth(1)
        .unwrap()
        .id();
    let first_in_second = tree.node_ref(second_comment).first_child().unwrap().id();
    tree.edit().detach(blocks[1]).unwrap();
    tree.edit()
        .insert_before(second_comment, Some(first_in_second), blocks[1])
        .unwrap();
    let r = Story::refresh(&prev, &tree, &[]).unwrap();
    let scratch = Story::scan(&name, StoryKind::Comments, &tree).unwrap();
    assert!(r.story.same_as(&scratch));
    assert_eq!(
        r.moved,
        [blocks[1]],
        "it is in the same place of the list, in another comment"
    );
    assert!(!r.fields_carried);
    assert_eq!(r.story.field_index(&tree), scratch.field_index(&tree));
    assert_eq!(
        r.field_starts_changed,
        [blocks[2]],
        "the paragraph after it now starts in the open field"
    );
}
