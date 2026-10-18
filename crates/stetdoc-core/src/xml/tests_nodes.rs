//! Tests for the structure of the tree under edits: the links between siblings, what moving a node
//! costs, how deep an insert may nest, and that a `NodeId` keeps meaning the node it was given for.

use super::*;
use crate::api::error::ErrorCode;
use std::collections::HashSet;
use std::time::Duration;

fn parse(s: &str) -> SpanTree {
    SpanTree::parse(
        "test.xml",
        Bytes::from(s.as_bytes().to_vec()),
        &Limits::default(),
    )
    .expect("parse")
}

fn out(t: &SpanTree) -> String {
    String::from_utf8(t.serialize()).expect("utf-8")
}

/// Every node reachable from `id`, parents before children.
fn all_ids(t: &SpanTree, id: NodeId) -> Vec<NodeId> {
    let mut found = Vec::new();
    let mut stack = vec![id];
    while let Some(id) = stack.pop() {
        found.push(id);
        let kids: Vec<NodeId> = t.node_ref(id).children().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev());
    }
    found
}

/// The children of `parent` by the links in both directions, which have to agree.
fn children_both_ways(t: &SpanTree, parent: NodeId) -> Vec<NodeId> {
    let forward: Vec<NodeId> = t.node_ref(parent).children().map(|c| c.id()).collect();
    let mut backward = Vec::new();
    let mut at = t.node_ref(parent).last_child();
    while let Some(n) = at {
        assert_eq!(n.parent().map(|p| p.id()), Some(parent), "parent link");
        backward.push(n.id());
        at = n.prev_sibling();
    }
    backward.reverse();
    assert_eq!(forward, backward, "next and prev links disagree");
    let first = t.node_ref(parent).first_child();
    assert_eq!(first.map(|c| c.id()), forward.first().copied());
    assert!(first.is_none_or(|c| c.prev_sibling().is_none()));
    let last = t.node_ref(parent).last_child();
    assert!(last.is_none_or(|c| c.next_sibling().is_none()));
    forward
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n.max(1) as u64).unwrap_or(0)
    }
}

#[test]
fn sibling_links_agree_with_a_plain_list_under_random_edits() {
    let mut t = parse("<r><a/><b/><c/><d/><e/></r>");
    let root = t.root().id();
    let mut model: Vec<NodeId> = children_both_ways(&t, root);
    let mut detached: Vec<NodeId> = Vec::new();
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for step in 0..2000 {
        let mut ed = t.edit();
        match rng.below(4) {
            0 if !model.is_empty() => {
                let node = model.remove(rng.below(model.len()));
                ed.detach(node).unwrap();
                detached.push(node);
            }
            1 if !detached.is_empty() => {
                let node = detached.swap_remove(rng.below(detached.len()));
                let at = rng.below(model.len() + 1);
                ed.insert_before(root, model.get(at).copied(), node)
                    .unwrap();
                model.insert(at, node);
            }
            2 => {
                let node = ed.create_element("", "n").unwrap();
                let at = rng.below(model.len() + 1);
                ed.insert_before(root, model.get(at).copied(), node)
                    .unwrap();
                model.insert(at, node);
            }
            _ if !model.is_empty() => {
                let copy = ed.clone_subtree(model[rng.below(model.len())]).unwrap();
                ed.append_child(root, copy).unwrap();
                model.push(copy);
            }
            _ => {}
        }
        if step % 50 == 0 {
            assert_eq!(children_both_ways(&t, root), model, "step {step}");
        }
    }
    assert_eq!(children_both_ways(&t, root), model);
    // Whatever order that left, the output parses to the same number of children.
    assert_eq!(parse(&out(&t)).root().children().count(), model.len());
}

#[test]
fn moving_a_node_in_a_long_list_takes_the_same_time_however_long_the_list_is() {
    // 10,000 moves in a list of 50,000 children took seconds when each had to find its neighbour
    // by walking the list.
    let src = format!("<r>{}</r>", "<a/>".repeat(50_000));
    let mut t = parse(&src);
    let root = t.root().id();
    let ids: Vec<NodeId> = t.root().children().map(|c| c.id()).collect();
    let done = std::sync::mpsc::channel();
    let tx = done.0;
    std::thread::spawn(move || {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let mut ed = t.edit();
        for _ in 0..10_000 {
            let node = ids[rng.below(ids.len())];
            let before = ids[rng.below(ids.len())];
            if node == before {
                continue;
            }
            ed.detach(node).unwrap();
            ed.insert_before(root, Some(before), node).unwrap();
        }
        let _ = tx.send(children_both_ways(&t, root).len());
    });
    let n = done
        .1
        .recv_timeout(Duration::from_secs(5))
        .expect("10,000 moves did not finish in time");
    assert_eq!(n, 50_000);
}

/// `<a>` nested `levels` deep, with `<z/>` as the only child of the innermost.
fn nested(levels: usize) -> String {
    format!("{}<z/>{}", "<a>".repeat(levels), "</a>".repeat(levels))
}

#[test]
fn an_insert_may_not_nest_deeper_than_the_limit_the_part_was_parsed_with() {
    // The depth limit is 256, counting the root element as 1.
    let max = Limits::default().max_xml_depth as usize;
    assert_eq!(max, 256);

    // A copy of a part that is 201 deep, under the 200-deep element it ends in, would be 401 deep.
    let src = nested(200);
    let mut t = parse(&src);
    let root = t.root().id();
    let leaf = *all_ids(&t, root)
        .iter()
        .rfind(|id| t.node_ref(**id).local_name() == "a")
        .unwrap();
    let mut ed = t.edit();
    let copy = ed.clone_subtree(root).unwrap();
    let e = ed.append_child(leaf, copy).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
    assert!(e.message().contains("401 levels deep"), "{e}");
    assert!(e.message().contains("limit of 256"), "{e}");
    // Nothing changed: the copy is still free, and the part is as it was.
    assert!(t.node_ref(copy).parent().is_none());
    assert_eq!(out(&t), src);
    assert!(!t.is_dirty());

    // Up to the limit is fine, and the part that is written parses again; one past it is not.
    let mut t = parse(&nested(254));
    let leaf = *all_ids(&t, t.root().id())
        .iter()
        .rfind(|id| t.node_ref(**id).local_name() == "a")
        .unwrap();
    let mut ed = t.edit();
    let at_255 = ed.create_element("", "b").unwrap();
    ed.append_child(leaf, at_255).unwrap();
    let at_256 = ed.create_element("", "c").unwrap();
    ed.append_child(at_255, at_256).unwrap();
    let at_257 = ed.create_element("", "d").unwrap();
    let e = ed.append_child(at_256, at_257).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
    // Text does not nest: it can go in the deepest element.
    let text = ed.create_text("deep").unwrap();
    ed.append_child(at_256, text).unwrap();
    assert!(
        SpanTree::parse(
            "again.xml",
            Bytes::from(out(&t).into_bytes()),
            &Limits::default()
        )
        .is_ok()
    );

    // Where the parent is not in the document yet, the subtree cannot be deeper than the limit
    // by itself, and a chain that is built from the top gets as far as the limit and no further.
    let mut t = parse("<r/>");
    let mut ed = t.edit();
    let mut top = ed.create_element("", "e").unwrap();
    let mut refused_at = None;
    for level in 2..=300 {
        let child = ed.create_element("", "e").unwrap();
        if ed.append_child(top, child).is_err() {
            refused_at = Some(level);
            break;
        }
        top = child;
    }
    assert_eq!(refused_at, Some(257));
    // The same chain, built from the bottom, is refused when the piece that would be too deep is
    // joined: no subtree can be taller than the limit.
    let mut t = parse("<r/>");
    let mut ed = t.edit();
    let mut bottom = ed.create_element("", "e").unwrap();
    let mut refused_at = None;
    for level in 2..=300 {
        let parent = ed.create_element("", "e").unwrap();
        if ed.append_child(parent, bottom).is_err() {
            refused_at = Some(level);
            break;
        }
        bottom = parent;
    }
    assert_eq!(refused_at, Some(257));
}

#[test]
fn a_node_id_is_never_reused_or_renumbered() {
    let src = "<w:document xmlns:w=\"urn:w\"><w:body><w:p><w:r><w:t>one</w:t></w:r></w:p>\
               <w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p>\
               </w:body></w:document>";
    let mut t = parse(src);
    let body = t.root().first_child().unwrap().id();
    /// Every id given out so far. Adding one that is there already is a reuse.
    #[derive(Default)]
    struct Seen(HashSet<NodeId>);
    impl Seen {
        fn add(&mut self, ids: &[NodeId], what: &str) {
            for id in ids {
                assert!(self.0.insert(*id), "{what}: {id:?} was already in use");
            }
        }
    }
    let mut seen = Seen::default();

    // Every node of a parsed tree has an id of its own.
    let parsed = all_ids(&t, t.document().id());
    seen.add(&parsed, "parse");
    let paragraphs: Vec<NodeId> = t.node_ref(body).children().map(|c| c.id()).collect();
    let second = paragraphs[1];
    let second_subtree = all_ids(&t, second);
    let describe = |t: &SpanTree, id: NodeId| {
        let n = t.node_ref(id);
        (
            n.kind(),
            n.name().to_owned(),
            n.text().into_owned(),
            n.parent().map(|p| p.id()),
        )
    };
    let before_detach: Vec<_> = second_subtree.iter().map(|id| describe(&t, *id)).collect();

    // A detached node and everything under it keep their ids and what they hold.
    t.edit().detach(second).unwrap();
    assert_eq!(all_ids(&t, second), second_subtree);
    let after_detach: Vec<_> = second_subtree.iter().map(|id| describe(&t, *id)).collect();
    assert_eq!(after_detach[0].3, None, "a detached node has no parent");
    assert_eq!(
        after_detach[1..],
        before_detach[1..],
        "what is under it did not change"
    );
    assert!(!all_ids(&t, t.document().id()).contains(&second));

    // New nodes made while it is detached never take an id it or anything under it has.
    let mut made = Vec::new();
    {
        let mut ed = t.edit();
        for i in 0..20 {
            let e = ed.create_element("urn:w", "p").unwrap();
            let x = ed.create_text(&format!("text {i}")).unwrap();
            ed.append_child(e, x).unwrap();
            if i % 2 == 0 {
                ed.insert_before(body, Some(paragraphs[0]), e).unwrap();
            }
            made.push(e);
            made.push(x);
        }
    }
    seen.add(&made, "create");

    // A copy gets new ids all through, and the original keeps its own.
    let copy = t.edit().clone_subtree(paragraphs[0]).unwrap();
    let copied = all_ids(&t, copy);
    assert_eq!(copied.len(), 4, "the copy of p, r, t and the text node");
    seen.add(&copied, "clone_subtree");
    t.edit().append_child(body, copy).unwrap();
    assert_eq!(all_ids(&t, paragraphs[0]).len(), 4);

    // A clone of the tree has the same ids for the same nodes.
    let mut twin = t.clone();
    let everything = all_ids(&t, t.document().id());
    assert_eq!(all_ids(&twin, twin.document().id()), everything);
    let before_the_clone = seen.0.clone();
    for id in &before_the_clone {
        assert_eq!(
            describe(&t, *id),
            describe(&twin, *id),
            "{id:?} in the clone"
        );
    }

    // After that each tree hands out ids on its own, and neither takes one that was in use.
    let mut fresh_here = Vec::new();
    let mut fresh_there = Vec::new();
    for _ in 0..10 {
        fresh_here.push(t.edit().create_element("", "x").unwrap());
        fresh_there.push(twin.edit().create_element("", "y").unwrap());
    }
    seen.add(&fresh_here, "create after the clone");
    for id in fresh_there.iter().chain(&fresh_here) {
        assert!(
            !before_the_clone.contains(id),
            "{id:?} was in use before the clone"
        );
    }
    assert_eq!(twin.node_ref(fresh_there[0]).name(), "y");
    assert_eq!(t.node_ref(fresh_here[0]).name(), "x");

    // The detached node goes back where it was taken from, and is the same node: its id and the
    // ids under it are the ones it had, and a node can be detached and made again many times
    // without any id coming back.
    t.edit().append_child(body, second).unwrap();
    assert_eq!(all_ids(&t, second), second_subtree);
    assert_eq!(t.node_ref(second).parent().map(|p| p.id()), Some(body));
    let mut churn = Vec::new();
    for _ in 0..50 {
        let mut ed = t.edit();
        ed.detach(second).unwrap();
        let e = ed.create_element("urn:w", "p").unwrap();
        ed.append_child(body, e).unwrap();
        ed.detach(e).unwrap();
        ed.append_child(body, second).unwrap();
        churn.push(e);
    }
    seen.add(&churn, "churn");
    assert_eq!(all_ids(&t, second), second_subtree);
}
