//! Tests of `SpanTree::edit_count`: it moves when the tree changes and at no other time, and a clone
//! has the count of the tree it was cloned from.

use super::*;
use crate::xml::NodeId;

const W: &str = "urn:w";

fn parse(s: &str) -> SpanTree {
    SpanTree::parse(
        "test.xml",
        Bytes::from(s.as_bytes().to_vec()),
        &Limits::default(),
    )
    .expect("parse")
}

fn tree() -> SpanTree {
    parse("<w:r xmlns:w=\"urn:w\" a=\"1\"><w:t>one</w:t></w:r>")
}

/// Asserts that the count went up since `last`, and returns it.
fn moved(t: &SpanTree, last: u64, what: &str) -> u64 {
    let now = t.edit_count();
    assert!(
        now > last,
        "{what} did not move the count ({last} to {now})"
    );
    now
}

#[test]
fn a_parsed_tree_has_count_zero_and_reading_it_does_not_move_it() {
    let t = tree();
    assert_eq!(t.edit_count(), 0);
    let _ = t.serialize();
    let _ = t.root().text_content();
    let _ = t.node_ref(t.root().id()).attrs().count();
    assert_eq!(t.edit_count(), 0);
    let mut t = t;
    // A borrow through the editor is not an edit.
    let _ = t.edit();
    assert_eq!(t.edit_count(), 0);
    assert!(!t.is_dirty());
}

#[test]
fn every_kind_of_change_moves_the_count() {
    let mut t = tree();
    let root = t.root().id();
    let text = t.root().first_child().unwrap().first_child().unwrap().id();
    let mut n = 0;
    t.edit().set_attr(root, "", "b", "2").unwrap();
    n = moved(&t, n, "adding an attribute");
    t.edit().set_attr(root, "", "b", "3").unwrap();
    n = moved(&t, n, "changing an attribute");
    assert!(t.edit().remove_attr(root, "", "b").unwrap());
    n = moved(&t, n, "removing an attribute");
    t.edit().set_text(text, "two").unwrap();
    n = moved(&t, n, "set_text");
    t.edit().replace_text_range(text, 0, 1, "T").unwrap();
    n = moved(&t, n, "replace_text_range");
    let element = t.edit().create_element(W, "p").unwrap();
    n = moved(&t, n, "create_element");
    let node = t.edit().create_text("x").unwrap();
    n = moved(&t, n, "create_text");
    t.edit().append_child(element, node).unwrap();
    n = moved(&t, n, "append_child");
    t.edit().insert_before(root, None, element).unwrap();
    n = moved(&t, n, "insert_before");
    t.edit().clone_subtree(element).unwrap();
    n = moved(&t, n, "clone_subtree");
    t.edit().detach(element).unwrap();
    n = moved(&t, n, "detach");
    t.edit().bind_namespace("urn:new", "n", false).unwrap();
    moved(&t, n, "bind_namespace");
    assert!(t.is_dirty());
}

#[test]
fn a_call_that_changes_nothing_does_not_move_the_count() {
    let mut t = tree();
    let root = t.root().id();
    let text = t.root().first_child().unwrap().first_child().unwrap().id();
    let detached = t.edit().create_element(W, "p").unwrap();
    let before = t.edit_count();
    // Refused edits.
    assert!(t.edit().set_text(root, "x").is_err());
    assert!(t.edit().set_text(text, "\u{1}").is_err());
    assert!(t.edit().set_attr(root, "", "c", "\u{1}").is_err());
    assert!(t.edit().replace_text_range(text, 5, 9, "x").is_err());
    // An attribute that is not there, a node that is already detached, a namespace that is already
    // bound.
    assert!(!t.edit().remove_attr(root, "", "absent").unwrap());
    t.edit().detach(detached).unwrap();
    t.edit().bind_namespace(W, "w", false).unwrap();
    assert_eq!(t.edit_count(), before);
    assert_eq!(
        String::from_utf8(t.serialize()).unwrap(),
        "<w:r xmlns:w=\"urn:w\" a=\"1\"><w:t>one</w:t></w:r>"
    );
}

#[test]
fn a_clone_has_the_count_it_was_cloned_at_and_counts_on_its_own() {
    let mut t = tree();
    let text = t.root().first_child().unwrap().first_child().unwrap().id();
    t.edit().set_text(text, "two").unwrap();
    let at = t.edit_count();
    let mut copy = t.clone();
    assert_eq!(copy.edit_count(), at, "the clone starts where the tree was");
    copy.edit().set_text(text, "three").unwrap();
    assert!(copy.edit_count() > at);
    assert_eq!(t.edit_count(), at, "and the original does not follow it");
    // Two trees with the same history have the same count.
    let mut twin = tree();
    twin.edit().set_text(text, "two").unwrap();
    assert_eq!(twin.edit_count(), at);
}

/// A small deterministic generator, so that a failing seed can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Every node under the document that is an element or a text node, in document order.
fn nodes(t: &SpanTree) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![t.document().id()];
    while let Some(id) = stack.pop() {
        out.push(id);
        let mut kids: Vec<NodeId> = t.node_ref(id).children().map(|c| c.id()).collect();
        kids.reverse();
        stack.extend(kids);
    }
    out
}

/// Whatever the editor changes shows in the serialization, so a call that changed the bytes of the
/// part, and did not move the count, is a path the count missed.
#[test]
fn no_call_of_the_editor_changes_the_part_without_moving_the_count() {
    let src = "<w:r xmlns:w=\"urn:w\" a=\"1\"><w:p><w:t>one</w:t><w:t>two</w:t></w:p><w:p b=\"2\"/></w:r>";
    for seed in 1..=200u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut t = parse(src);
        let mut spare: Vec<NodeId> = Vec::new();
        for step in 0..30 {
            let all = nodes(&t);
            let pick = |rng: &mut Rng| all[rng.below(all.len())];
            let before_bytes = t.serialize();
            let before_count = t.edit_count();
            let which = rng.below(12);
            let mut ed = t.edit();
            let _ = match which {
                0 => ed.set_attr(pick(&mut rng), "", "k", "v"),
                1 => ed.set_attr(pick(&mut rng), W, "w", "x"),
                2 => ed.remove_attr(pick(&mut rng), "", "a").map(|_| ()),
                3 => ed.set_text(pick(&mut rng), "text"),
                4 => ed.replace_text_range(pick(&mut rng), 0, 1, "Z"),
                5 => ed.create_element(W, "p").map(|n| spare.push(n)),
                6 => ed.create_text("t").map(|n| spare.push(n)),
                7 => match spare.pop() {
                    Some(n) => ed.insert_before(pick(&mut rng), None, n),
                    None => Ok(()),
                },
                8 => ed.detach(pick(&mut rng)),
                9 => ed.clone_subtree(pick(&mut rng)).map(|n| spare.push(n)),
                10 => ed
                    .bind_namespace("urn:other", "o", rng.below(2) == 0)
                    .map(|_| ()),
                _ => ed.append_child(pick(&mut rng), spare.pop().unwrap_or(pick(&mut rng))),
            };
            if t.serialize() != before_bytes {
                assert!(
                    t.edit_count() > before_count,
                    "seed {seed} step {step} call {which}: the part changed and the count did not"
                );
            }
        }
    }
}
