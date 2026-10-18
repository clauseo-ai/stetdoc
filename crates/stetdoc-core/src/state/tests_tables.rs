//! What a table and a row own, and when the commit stamps them: a changed list of rows or cells is a
//! change of the table or the row, an edit inside a cell is not.

use super::tests::*;
use super::tests_edit::refs;
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::Claim;

/// A table of two rows with one cell each, and a paragraph after it.
const TABLE: &str = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>z</w:t></w:r></w:p>";

/// The same with two cells in the first row.
const WIDE: &str = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>w</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>z</w:t></w:r></w:p>";

/// The refs that a batch written at `claim` may still name.
fn kept(next: &State, claim: &BaseToken) -> Vec<String> {
    let mut all: Vec<String> = refs(next).into_iter().filter(|r| r != "-").collect();
    all.sort();
    all.into_iter()
        .filter(|r| match next.begin().resolve(Claim::Base(claim), r) {
            Ok(_) => true,
            Err(e) => {
                assert_eq!(e.code(), ErrorCode::StaleRef, "{r}: {e}");
                false
            }
        })
        .collect()
}

/// Inserts a copy of the second row after it, and returns the key of the table.
fn add_row(tx: &mut Tx<'_>, table: &BlockKey, row: &BlockKey) {
    let tree = tx.part_mut(MAIN).unwrap();
    let copy = tree.edit().clone_subtree(row.node()).unwrap();
    tree.edit().insert_before(table.node(), None, copy).unwrap();
}

#[test]
fn touching_a_table_for_a_changed_list_of_rows_stamps_it() {
    let s = state(TABLE);
    assert_eq!(refs(&s), ["t1", "r1", "p1", "r2", "p2", "p3"]);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    add_row(&mut tx, &keys[0], &keys[3]);
    // The list of rows is what the table owns, so the operation touches the table, and the audit takes
    // that without a word: nothing in the contract of `touch` refuses a table.
    tx.touch(&keys[0]);
    assert_eq!(tx.touched(), [keys[0].clone()]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        refs(&next),
        ["t1", "r1", "p1", "r2", "p2", "r3", "p4", "p3"],
        "the new row and its paragraph get the next numbers of their kinds"
    );
    assert_eq!(committed.minted.len(), 2);
    assert_eq!(
        kept(&next, &v0),
        ["p1", "p2", "p3", "r1", "r2"],
        "the table is stale for a batch written before, the new blocks need no claim, and every other block is as it was"
    );
}

#[test]
fn a_table_whose_rows_changed_is_stamped_though_no_operation_touched_it() {
    // Adding a row.
    let s = state(TABLE);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    add_row(&mut tx, &keys[0], &keys[3]);
    assert!(tx.touched().is_empty());
    let (next, _) = tx.commit().unwrap();
    assert_eq!(
        kept(&next, &v0),
        ["p1", "p2", "p3", "r1", "r2"],
        "t1 is stale"
    );
    // Removing a row.
    let mut tx = s.begin();
    detach(&mut tx, &keys[3]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["t1", "r1", "p1", "p3"]);
    assert_eq!(committed.retired.len(), 2, "the row and its paragraph");
    assert_eq!(kept(&next, &v0), ["p1", "p3", "r1"], "t1 is stale");
    // Reordering the rows.
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let first = tree.node_ref(keys[1].node());
        let table = first.parent().unwrap().id();
        let after = tree.node_ref(keys[3].node()).next_sibling().map(|n| n.id());
        tree.edit().detach(keys[1].node()).unwrap();
        tree.edit()
            .insert_before(table, after, keys[1].node())
            .unwrap();
    }
    let (next, _) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["t1", "r2", "p2", "r1", "p1", "p3"]);
    assert!(
        !kept(&next, &v0).contains(&"t1".to_owned()),
        "t1 is stale after a reorder"
    );
}

#[test]
fn a_row_whose_cells_changed_is_stamped_and_its_table_is_not() {
    let s = state(WIDE);
    assert_eq!(refs(&s), ["t1", "r1", "p1", "p2", "r2", "p3", "p4"]);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    {
        // The second cell of the first row goes, and its paragraph with it.
        let tree = tx.part_mut(MAIN).unwrap();
        let cell = tree.node_ref(keys[3].node()).parent().unwrap().id();
        tree.edit().detach(cell).unwrap();
    }
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["t1", "r1", "p1", "r2", "p3", "p4"]);
    assert_eq!(committed.retired.len(), 1, "the paragraph of the cell");
    assert_eq!(
        kept(&next, &v0),
        ["p1", "p3", "p4", "r2", "t1"],
        "the row lost a cell, and nothing else of what it contains or what contains it changed"
    );
}

#[test]
fn an_edit_inside_a_cell_stamps_neither_the_table_nor_the_row() {
    let s = state(TABLE);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    edit_first_text(&mut tx, "changed");
    tx.touch(&keys[2]);
    let (next, _) = tx.commit().unwrap();
    assert_eq!(
        kept(&next, &v0),
        ["p2", "p3", "r1", "r2", "t1"],
        "only the paragraph that was touched is stale"
    );
}
