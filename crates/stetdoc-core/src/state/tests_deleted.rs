//! Which paragraphs are tracked-deleted, and what state does about each shape. A paragraph whose
//! mark is deleted and whose final text is empty is a deleted block. A paragraph whose content is
//! all deleted but whose mark is live (the last paragraph of a container, or one that keeps a section
//! break) is a live block that can be edited, and a paragraph whose mark is deleted but that still
//! has live text is a pending merge. In every shape the element is in the tree, so the ref is live.
//! What differs is what `revisions` allows, and the stamps, which these tests check.

use super::tests::*;
use super::tests_edit::refs;
use super::*;
use crate::api::error::ErrorCode;
use crate::api::view::Mode;
use crate::refs::Claim;

const STAMP: [(&str, &str); 3] = [
    ("author", "a"),
    ("date", "2024-01-01T00:00:00Z"),
    ("id", "1"),
];

/// Deletes the content of a paragraph by a tracked change, as Word writes it: its run goes into a
/// `w:del`, with the text in `w:delText`. With `mark`, the paragraph mark is deleted too.
fn tracked_delete(tx: &mut Tx<'_>, key: &BlockKey, text: &str, mark: bool) {
    let tree = tx.part_mut(MAIN).unwrap();
    let para = key.node();
    let run = tree.node_ref(para).last_child().map(|n| n.id()).unwrap();
    let mut e = tree.edit();
    let del = e.create_element(W, "del").unwrap();
    for (name, value) in STAMP {
        e.set_attr(del, W, name, value).unwrap();
    }
    let r = e.create_element(W, "r").unwrap();
    let t = e.create_element(W, "delText").unwrap();
    let content = e.create_text(text).unwrap();
    e.append_child(t, content).unwrap();
    e.append_child(r, t).unwrap();
    e.append_child(del, r).unwrap();
    e.insert_before(para, Some(run), del).unwrap();
    e.detach(run).unwrap();
    if mark {
        let ppr = e.create_element(W, "pPr").unwrap();
        let rpr = e.create_element(W, "rPr").unwrap();
        let mark = e.create_element(W, "del").unwrap();
        for (name, value) in STAMP {
            e.set_attr(mark, W, name, value).unwrap();
        }
        e.append_child(rpr, mark).unwrap();
        e.append_child(ppr, rpr).unwrap();
        let first = tree.node_ref(para).first_child().map(|n| n.id());
        tree.edit().insert_before(para, first, ppr).unwrap();
    }
}

fn text(s: &State, i: usize, mode: Mode) -> String {
    let main = id(s, MAIN);
    let story = s.parts().story(main).unwrap();
    let tree = s.parts().tree(main).unwrap();
    story.paragraph(tree, i).unwrap().text(mode)
}

/// The refs that a batch written at `claim` may still name.
fn kept(next: &State, claim: &BaseToken) -> Vec<String> {
    refs(next)
        .into_iter()
        .filter(|r| match next.begin().resolve(Claim::Base(claim), r) {
            Ok(_) => true,
            Err(e) => {
                assert_eq!(e.code(), ErrorCode::StaleRef, "{r}: {e}");
                false
            }
        })
        .collect()
}

#[test]
fn a_paragraph_whose_mark_and_content_are_deleted_is_absent_in_final_and_keeps_its_ref() {
    let s = state(&paragraphs(&["a", "b", "c", "d"]));
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    tracked_delete(&mut tx, &keys[1], "b", true);
    tx.touch(&keys[1]);
    let (next, committed) = tx.commit().unwrap();
    // The block is in the tree, so it keeps its ref and nothing is minted or retired.
    assert_eq!(refs(&next), ["p1", "p2", "p3", "p4"]);
    assert!(committed.minted.is_empty() && committed.retired.is_empty());
    assert_eq!(next.resolve(Claim::Implied, "p2").unwrap(), keys[1]);
    // It is absent in the final view and there in the original one.
    assert_eq!(text(&next, 1, Mode::Final), "");
    assert_eq!(text(&next, 1, Mode::Original), "b");
    // The deleted mark changes what the next paragraph is part of, so p3 is stamped with it.
    assert_eq!(kept(&next, &v0), ["p1", "p4"]);
}

#[test]
fn a_paragraph_whose_content_is_deleted_but_whose_mark_is_live_stays_a_live_block() {
    let s = state(&paragraphs(&["a", "b", "c", "d"]));
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    tracked_delete(&mut tx, &keys[1], "b", false);
    tx.touch(&keys[1]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(refs(&next), ["p1", "p2", "p3", "p4"]);
    assert!(committed.minted.is_empty() && committed.retired.is_empty());
    assert_eq!(text(&next, 1, Mode::Final), "");
    assert_eq!(text(&next, 1, Mode::Original), "b");
    // The mark did not change, so no other block did: only the touched paragraph is stale.
    assert_eq!(kept(&next, &v0), ["p1", "p3", "p4"]);
    // It can be edited: new text goes after the deleted text, and the block keeps its ref.
    let v1 = next.base_token();
    let mut tx = next.begin();
    assert_eq!(tx.resolve(Claim::Base(&v1), "p2").unwrap(), keys[1]);
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let mut e = tree.edit();
        let r = e.create_element(W, "r").unwrap();
        let t = e.create_element(W, "t").unwrap();
        let content = e.create_text("new").unwrap();
        e.append_child(t, content).unwrap();
        e.append_child(r, t).unwrap();
        e.append_child(keys[1].node(), r).unwrap();
    }
    tx.touch(&keys[1]);
    let (after, committed) = tx.commit().unwrap();
    assert_eq!(refs(&after), ["p1", "p2", "p3", "p4"]);
    assert!(committed.minted.is_empty() && committed.retired.is_empty());
    assert_eq!(text(&after, 1, Mode::Final), "new");
    assert_eq!(
        text(&after, 1, Mode::Original),
        "bnew",
        "the new text is not tracked, so it is in both views"
    );
}

#[test]
fn a_paragraph_whose_mark_is_deleted_but_has_live_text_is_a_pending_merge_and_stays_live() {
    let s = state(&paragraphs(&["a", "b", "c", "d"]));
    let keys = body_keys(&s);
    let v0 = s.base_token();
    let mut tx = s.begin();
    {
        // Only the mark: b keeps its text.
        let tree = tx.part_mut(MAIN).unwrap();
        let mut e = tree.edit();
        let ppr = e.create_element(W, "pPr").unwrap();
        let rpr = e.create_element(W, "rPr").unwrap();
        let mark = e.create_element(W, "del").unwrap();
        for (name, value) in STAMP {
            e.set_attr(mark, W, name, value).unwrap();
        }
        e.append_child(rpr, mark).unwrap();
        e.append_child(ppr, rpr).unwrap();
        let first = tree.node_ref(keys[1].node()).first_child().map(|n| n.id());
        tree.edit()
            .insert_before(keys[1].node(), first, ppr)
            .unwrap();
    }
    tx.touch(&keys[1]);
    let (next, committed) = tx.commit().unwrap();
    assert_eq!(
        refs(&next),
        ["p1", "p2", "p3", "p4"],
        "nothing is declared or retired"
    );
    assert!(committed.minted.is_empty() && committed.retired.is_empty());
    assert_eq!(text(&next, 1, Mode::Final), "b", "it still has live text");
    // The join with the next paragraph is pending, so p3 is stamped with p2.
    assert_eq!(kept(&next, &v0), ["p1", "p4"]);
}
