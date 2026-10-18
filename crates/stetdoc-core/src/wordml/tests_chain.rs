//! Chains of refreshes as `state` makes them, with the exact list of what was edited in place, over a
//! body and over a comments part, and the check that matters for the seam: whatever a paragraph reads
//! differently after an edit is something `state` will stamp.
//!
//! `state` keeps each story it refreshed and refreshes that one on the next edit, so a stale value is
//! never dropped by a fresh scan. It passes the blocks whose own bytes changed, found by comparing them
//! before and after, and stamps those, the blocks that were added or moved, and the two lists `refresh`
//! reports for text that changes while the bytes do not (`context_changed` and
//! `field_starts_changed`). This test makes the same comparison, with a hash of each paragraph's
//! subtree in place of bytes, and then compares what every paragraph reads, in both modes, with what it
//! read before. Edits are the chaos of `tests_scan_edit.rs`, which puts any node under any element, and
//! in the comments part moves paragraphs between comments, which a block list alone does not show.

use super::testing::part;
use super::tests_gen::Gen;
use super::tests_gen_data::Rng;
use super::tests_scan_edit::chaos_in;
use super::*;
use crate::xml::{NodeKind, NodeRef, SpanTree};
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// A hash of a node and everything below it: names, attributes (not namespace declarations) and
/// text, with the shape of the nesting. Two equal subtrees hash equal, and an edit inside one does not.
fn signature(tree: &SpanTree, node: NodeId) -> u64 {
    let mut h = DefaultHasher::new();
    let open = |h: &mut DefaultHasher, n: &NodeRef<'_>| match n.kind() {
        NodeKind::Element => {
            n.namespace().hash(h);
            n.local_name().hash(h);
            for a in n.attrs().filter(|a| !a.is_namespace_declaration()) {
                (a.namespace(), a.local_name(), a.value()).hash(h);
            }
            h.write_u8(1);
        }
        NodeKind::Text | NodeKind::CData => n.text().hash(h),
        _ => {}
    };
    let root = tree.node_ref(node);
    open(&mut h, &root);
    let mut stack = vec![root.children()];
    while let Some(top) = stack.last_mut() {
        match top.next() {
            None => {
                stack.pop();
                h.write_u8(2);
            }
            Some(c) => {
                open(&mut h, &c);
                if c.is_element() {
                    stack.push(c.children());
                }
            }
        }
    }
    h.finish()
}

/// What `touched` should list after an edit: the paragraphs of the previous story whose own content
/// changed, found by comparing the tree before and after.
fn exact_touched(before: &SpanTree, after: &SpanTree, prev: &Story) -> Vec<NodeId> {
    prev.blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph)
        .map(|e| e.node)
        .filter(|&n| signature(before, n) != signature(after, n))
        .collect()
}

/// What a paragraph reads, in both modes, with the kind, width, field and revision state of its items.
type Reading = (String, String, Vec<(ItemKind, u32, u8, u8)>);

fn reading(story: &Story, tree: &SpanTree, node: NodeId) -> Option<Reading> {
    let p = story.paragraph(tree, story.index_of(node)?)?;
    let m = p.map();
    Some((
        m.text(Mode::Final).into_owned(),
        m.text(Mode::Original).into_owned(),
        m.items()
            .iter()
            .map(|i| (i.kind, i.len, i.flags & FIELD_RESULT, i.vis.0))
            .collect(),
    ))
}

/// A part to edit: its tree, what kind of story it is, its name, and the element chaos works below.
struct Subject {
    tree: SpanTree,
    kind: StoryKind,
    name: Arc<str>,
    container: NodeId,
}

fn body(g: &mut Gen) -> Subject {
    let tree = super::testing::document(&g.body(4).xml);
    let container = tree.root().first_child().unwrap().id();
    Subject {
        tree,
        kind: StoryKind::Body,
        name: Arc::from("word/document.xml"),
        container,
    }
}

/// Three comments with generated blocks in them, so that fields, revisions and wrappers are there
/// to be moved from one comment to another.
fn comments(g: &mut Gen) -> Subject {
    let content: String = (1..=3)
        .map(|id| {
            format!(
                "<w:comment w:id=\"{id}\" w:author=\"a\">{}</w:comment>",
                g.body(3).xml
            )
        })
        .collect();
    let tree = part("word/comments.xml", "comments", &content);
    let container = tree.root().id();
    Subject {
        tree,
        kind: StoryKind::Comments,
        name: Arc::from("word/comments.xml"),
        container,
    }
}

/// Moves a paragraph to the start or the end of a comment, which may be another one. It is the edit
/// that puts the last paragraph of a comment before the first of the next, where the block list and
/// the paragraph's own bytes are the same and the fields open where it starts are not.
fn move_to_a_comment(s: &mut Subject, rng: &mut Rng) {
    let Ok(story) = Story::scan(&s.name, s.kind, &s.tree) else {
        return;
    };
    let paragraphs: Vec<NodeId> = story
        .blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph)
        .map(|e| e.node)
        .collect();
    let comments: Vec<NodeId> = s
        .tree
        .node_ref(s.container)
        .child_elements()
        .filter(|c| c.local_name() == "comment")
        .map(|c| c.id())
        .collect();
    if paragraphs.is_empty() || comments.is_empty() {
        return;
    }
    let (p, c) = (*rng.pick(&paragraphs), *rng.pick(&comments));
    let first = s.tree.node_ref(c).first_child().map(|n| n.id());
    let mut edit = s.tree.edit();
    if edit.detach(p).is_ok() {
        let before = if rng.chance(50) { first } else { None };
        if edit.insert_before(c, before, p).is_err() {
            let _ = edit.detach(p);
        }
    }
}

/// How many sequences each variant runs. Three thousand of each, 150,000 edits in all, were run once
/// when the sharing rule of `refresh.rs` last changed, with no difference.
const SEEDS: u64 = 250;

#[test]
fn chained_refreshes_with_exact_touched_lists_equal_a_scan_and_stamp_every_change_of_reading() {
    let (mut shared, mut rebuilt, mut seam_only, mut changed_readings) = (0, 0, 0, 0);
    for seed in 1..=SEEDS {
        for variant in 0..2u64 {
            let mut rng = Rng::new(seed * 2 + variant);
            let mut g = Gen::new(seed);
            let mut s = if variant == 0 {
                body(&mut g)
            } else {
                comments(&mut g)
            };
            let mut prev = Story::scan(&s.name, s.kind, &s.tree).unwrap();
            for step in 0..25 {
                let at = format!("seed {seed}, variant {variant}, step {step}");
                let before = s.tree.clone();
                // `state` forces the field pass of the story it refreshes, so what a paragraph read
                // before is known. A story that never ran it has nothing to compare with, and
                // `refresh` then reports no start that changed, so only the equality is checked.
                let primed = rng.chance(75);
                if primed {
                    let _ = prev.fields(&before);
                }
                if variant == 1 && rng.chance(40) {
                    move_to_a_comment(&mut s, &mut rng);
                } else {
                    chaos_in(&mut s.tree, &mut rng, s.container);
                }
                let scratch = Story::scan(&s.name, s.kind, &s.tree).unwrap();
                let touched = exact_touched(&before, &s.tree, &prev);
                let r = Story::refresh(&prev, &s.tree, &touched).unwrap();
                assert!(r.story.same_as(&scratch), "{at}");
                assert_eq!(
                    r.story.field_index(&s.tree),
                    scratch.field_index(&s.tree),
                    "{at}"
                );
                if r.fields_carried {
                    shared += 1;
                } else {
                    rebuilt += 1;
                }
                // what `state` stamps, and what each paragraph reads now and read before
                let by_bytes: HashSet<NodeId> = touched
                    .iter()
                    .chain(&r.added)
                    .chain(&r.moved)
                    .copied()
                    .collect();
                let by_seam: HashSet<NodeId> = r
                    .context_changed
                    .iter()
                    .chain(&r.field_starts_changed)
                    .copied()
                    .collect();
                for e in scratch.blocks() {
                    if e.kind != BlockKind::Paragraph || prev.index_of(e.node).is_none() {
                        continue;
                    }
                    let (was, is) = (
                        reading(&prev, &before, e.node),
                        reading(&r.story, &s.tree, e.node),
                    );
                    if was == is || !primed {
                        continue;
                    }
                    changed_readings += 1;
                    let stamped_by_bytes = by_bytes.contains(&e.node);
                    assert!(
                        stamped_by_bytes || by_seam.contains(&e.node),
                        "{at}: a paragraph reads differently and nothing says so\n  was {was:?}\n  is  {is:?}"
                    );
                    if !stamped_by_bytes {
                        seam_only += 1;
                    }
                }
                prev = r.story;
            }
        }
    }
    assert!(
        shared > 500 && rebuilt > 500,
        "{shared} shared, {rebuilt} rebuilt"
    );
    assert!(
        changed_readings > 500,
        "{changed_readings} readings changed"
    );
    assert!(
        seam_only > 20,
        "{seam_only} readings changed with no change to the paragraph's own bytes, found by the seam"
    );
}
