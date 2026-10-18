//! Wrappers that change what a block reads without changing its bytes or its place: the audit
//! stamps the blocks that `Story::refresh` reports in `context_changed`, so a batch that was written
//! before the change and names one of them is `stale_ref`.

use super::tests::*;
use super::tests_edit::refs;
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::Claim;
use crate::wordml::Story;

const TABLE: &str = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr><w:tr><w:tc><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:t>z</w:t></w:r></w:p>";

/// Which of the refs a batch written at `claim` may still name, as `ref: true or false`.
fn fresh(next: &State, claim: &BaseToken, tokens: &[&str]) -> Vec<(String, bool)> {
    tokens
        .iter()
        .map(|t| {
            let ok = match next.begin().resolve(Claim::Base(claim), t) {
                Ok(_) => true,
                Err(e) => {
                    assert_eq!(e.code(), ErrorCode::StaleRef, "{t}: {e}");
                    false
                }
            };
            ((*t).to_owned(), ok)
        })
        .collect()
}

/// What `refresh` finds between the base's story and the transaction's tree for the main part.
fn refreshed(tx: &Tx<'_>) -> crate::wordml::Refreshed {
    let prev = tx.base().parts().story(id(tx.base(), MAIN)).unwrap();
    let tree = tx.parts().tree(id(tx.base(), MAIN)).unwrap();
    Story::refresh(&prev, tree, &[]).unwrap()
}

/// Wraps the element in a block-level content control: `w:sdt/w:sdtContent` takes its place.
fn wrap_in_control(tx: &mut Tx<'_>, key: &BlockKey) {
    let tree = tx.part_mut(MAIN).unwrap();
    let el = key.node();
    let at = tree.node_ref(el);
    let (parent, next) = (at.parent().unwrap().id(), at.next_sibling().map(|n| n.id()));
    let mut e = tree.edit();
    let sdt = e.create_element(W, "sdt").unwrap();
    let content = e.create_element(W, "sdtContent").unwrap();
    e.append_child(sdt, content).unwrap();
    e.detach(el).unwrap();
    e.append_child(content, el).unwrap();
    e.insert_before(parent, next, sdt).unwrap();
}

/// Puts `<w:trPr><w:del .../></w:trPr>` first in a row, the way a tracked row deletion is written.
fn delete_row_by_revision(tx: &mut Tx<'_>, row: &BlockKey) {
    let tree = tx.part_mut(MAIN).unwrap();
    let first = tree.node_ref(row.node()).first_child().map(|n| n.id());
    let mut e = tree.edit();
    let pr = e.create_element(W, "trPr").unwrap();
    let del = e.create_element(W, "del").unwrap();
    for (name, value) in [
        ("id", "7"),
        ("author", "a"),
        ("date", "2024-01-01T00:00:00Z"),
    ] {
        e.set_attr(del, W, name, value).unwrap();
    }
    e.append_child(pr, del).unwrap();
    e.insert_before(row.node(), first, pr).unwrap();
}

#[test]
fn wrapping_a_paragraph_in_a_content_control_stamps_it() {
    let s = state(&paragraphs(&["a", "b", "c"]));
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    wrap_in_control(&mut tx, &keys[1]);
    // Nothing was touched, and the block's own bytes and place are the same.
    assert!(tx.touched().is_empty());
    let r = refreshed(&tx);
    assert!(r.context_changed.contains(&keys[1].node()), "{r:?}");
    assert!(r.added.is_empty() && r.removed.is_empty(), "{r:?}");
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        refs(&next),
        ["p1", "p2", "p3"],
        "the ref follows the element"
    );
    assert!(committed.minted.is_empty() && committed.retired.is_empty());
    assert_eq!(
        fresh(&next, &v0, &["p1", "p2", "p3"]),
        [
            ("p1".into(), true),
            ("p2".into(), false),
            ("p3".into(), true)
        ]
    );
    // Taking it out again is a change too.
    let v1 = next.base_token();
    let keys = body_keys(&next);
    let mut tx = next.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let at = tree.node_ref(keys[1].node());
        let sdt = at.parent().unwrap().parent().unwrap().id();
        let parent = tree.node_ref(sdt).parent().unwrap().id();
        let mut e = tree.edit();
        e.detach(keys[1].node()).unwrap();
        e.insert_before(parent, Some(sdt), keys[1].node()).unwrap();
        e.detach(sdt).unwrap();
    }
    let (out, _) = tx.commit().unwrap();
    assert_eq!(refs(&out), ["p1", "p2", "p3"]);
    assert_eq!(
        fresh(&out, &v1, &["p1", "p2", "p3"]),
        [
            ("p1".into(), true),
            ("p2".into(), false),
            ("p3".into(), true)
        ]
    );
}

#[test]
fn a_row_that_gains_a_deleted_row_revision_stamps_the_paragraphs_in_it() {
    let s = state(TABLE);
    // The blocks: the table, its two rows with a paragraph each, and a paragraph after it.
    assert_eq!(refs(&s), ["t1", "r1", "p1", "r2", "p2", "p3"]);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    delete_row_by_revision(&mut tx, &keys[1]);
    // The row's own parts changed, so the operation touches the row, and nothing else.
    tx.touch(&keys[1]);
    let r = refreshed(&tx);
    assert_eq!(
        r.context_changed,
        [keys[2].node()],
        "the paragraph in the row: {r:?}"
    );
    let (next, _) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["t1", "r1", "p1", "r2", "p2", "p3"]);
    assert_eq!(
        fresh(&next, &v0, &["t1", "r1", "p1", "r2", "p2", "p3"]),
        [
            ("t1".into(), true),
            ("r1".into(), false),
            ("p1".into(), false),
            ("r2".into(), true),
            ("p2".into(), true),
            ("p3".into(), true)
        ],
        "the touched row and the paragraph it holds, and no other block"
    );
}

#[test]
fn a_cell_that_gains_a_deleted_cell_revision_stamps_the_paragraphs_in_it() {
    let s = state(TABLE);
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let cell = tree.node_ref(keys[4].node()).parent().unwrap().id();
        let first = tree.node_ref(cell).first_child().map(|n| n.id());
        let mut e = tree.edit();
        let pr = e.create_element(W, "tcPr").unwrap();
        let del = e.create_element(W, "cellDel").unwrap();
        for (name, value) in [
            ("id", "8"),
            ("author", "a"),
            ("date", "2024-01-01T00:00:00Z"),
        ] {
            e.set_attr(del, W, name, value).unwrap();
        }
        e.append_child(pr, del).unwrap();
        e.insert_before(cell, first, pr).unwrap();
    }
    // The row owns its cells' properties, so the operation touches the row.
    tx.touch(&keys[3]);
    let (next, _) = tx.commit().unwrap();
    assert_eq!(
        fresh(&next, &v0, &["t1", "r1", "p1", "r2", "p2", "p3"]),
        [
            ("t1".into(), true),
            ("r1".into(), true),
            ("p1".into(), true),
            ("r2".into(), false),
            ("p2".into(), false),
            ("p3".into(), true)
        ]
    );
}
