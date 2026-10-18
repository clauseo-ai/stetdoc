//! A tree the editor edited and re-scanned (`docs/design/wordml.md` 3.2): "which must never fail when
//! the first scan succeeded". First the three edits the design names (clone a paragraph, move a row,
//! delete a block), then seeded sequences of edits that put any node under any element, which the
//! `xml` editor allows and the scan has to read all the same. After each edit the body scans, a refresh
//! equals a scan, and every paragraph the scan lists passes the invariants of 5.2.

use super::names::W_T as W;
use super::testing::document;
use super::tests_gen::Gen;
use super::tests_gen_data::Rng;
use super::tests_gen_laws::check_laws;
use super::*;
use crate::xml::{NodeKind, SpanTree};
use std::sync::Arc;

fn scan(tree: &SpanTree) -> Result<Story, crate::api::error::StetError> {
    Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree)
}

pub(super) fn body_of(tree: &SpanTree) -> NodeId {
    tree.root().first_child().unwrap().id()
}

const DOC: &str = "<w:p><w:r><w:t>a</w:t></w:r></w:p>\
<w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>\
<w:sdt><w:sdtContent><w:p/></w:sdtContent></w:sdt><w:p/>";

#[test]
fn a_cloned_paragraph_a_moved_row_and_a_deleted_block_all_scan_again() {
    let mut tree = document(DOC);
    let first = scan(&tree).unwrap();
    let blocks: Vec<NodeId> = first.blocks().iter().map(|e| e.node).collect();
    // blocks: p, tbl, tr, p, tr, p, p, p
    let body = body_of(&tree);
    // clone a paragraph and put the copy at the end of the body
    let copy = tree.edit().clone_subtree(blocks[0]).unwrap();
    tree.edit().append_child(body, copy).unwrap();
    let s = scan(&tree).unwrap();
    assert_eq!(s.blocks().len(), 9);
    assert_ne!(
        s.blocks()[8].node,
        blocks[0],
        "a clone is a new block with a new key"
    );
    assert_eq!(s.blocks()[8].kind, BlockKind::Paragraph);
    // move the second row to the front of the first
    let (row1, row2) = (blocks[2], blocks[4]);
    let table = blocks[1];
    tree.edit().detach(row2).unwrap();
    tree.edit().insert_before(table, Some(row1), row2).unwrap();
    let s = scan(&tree).unwrap();
    let kinds: Vec<BlockKind> = s.blocks().iter().map(|e| e.kind).collect();
    assert_eq!(kinds.len(), 9);
    assert_eq!(
        s.blocks()[2].node,
        row2,
        "the moved row keeps its key and now comes first"
    );
    assert_eq!(s.blocks()[2].parent, Some(1));
    // delete the table, then every block
    tree.edit().detach(table).unwrap();
    assert_eq!(scan(&tree).unwrap().blocks().len(), 4);
    for e in scan(&tree)
        .unwrap()
        .blocks()
        .iter()
        .filter(|e| e.parent.is_none())
    {
        tree.edit().detach(e.node).unwrap();
    }
    let s = scan(&tree).unwrap();
    assert!(s.blocks().is_empty() && s.anomalies().is_empty());
    assert!(s.fingerprints(&tree).is_empty() && s.fields(&tree).is_empty());
}

/// Every node below `body`, text nodes included, in document order.
pub(super) fn below(tree: &SpanTree, body: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = tree.node_ref(body).children().map(|c| c.id()).collect();
    stack.reverse();
    while let Some(id) = stack.pop() {
        out.push(id);
        let kids: Vec<NodeId> = tree.node_ref(id).children().map(|c| c.id()).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

const NAMES: [&str; 14] = [
    "p",
    "tbl",
    "tr",
    "tc",
    "sdt",
    "sdtContent",
    "r",
    "t",
    "ins",
    "del",
    "bookmarkStart",
    "fldSimple",
    "hyperlink",
    "pPr",
];

/// One edit anywhere in the body. An edit the editor refuses (a node under itself) is no edit.
fn chaos(tree: &mut SpanTree, rng: &mut Rng) {
    let body = body_of(tree);
    chaos_in(tree, rng, body);
}

/// One edit anywhere below `body`, which is the container of the part's blocks. An edit the editor
/// refuses (a node under itself) is no edit.
pub(super) fn chaos_in(tree: &mut SpanTree, rng: &mut Rng, body: NodeId) {
    let all = below(tree, body);
    let elements: Vec<NodeId> = std::iter::once(body)
        .chain(
            all.iter()
                .copied()
                .filter(|&n| tree.node_ref(n).kind() == NodeKind::Element),
        )
        .collect();
    let pick = |rng: &mut Rng, from: &[NodeId]| from[rng.below(from.len())];
    let place = |tree: &mut SpanTree, rng: &mut Rng, node: NodeId| {
        let parent = pick(rng, &elements);
        let kids: Vec<NodeId> = tree.node_ref(parent).children().map(|c| c.id()).collect();
        let before = (!kids.is_empty() && rng.chance(70)).then(|| kids[rng.below(kids.len())]);
        if tree.edit().insert_before(parent, before, node).is_err() {
            let _ = tree.edit().detach(node);
        }
    };
    if all.is_empty() {
        let el = tree
            .edit()
            .create_element(W, NAMES[rng.below(NAMES.len())])
            .unwrap();
        place(tree, rng, el);
        return;
    }
    let victim = pick(rng, &all);
    match rng.below(5) {
        0 => {
            tree.edit().detach(victim).unwrap();
        }
        1 => {
            tree.edit().detach(victim).unwrap();
            place(tree, rng, victim);
        }
        2 => {
            let copy = tree.edit().clone_subtree(victim).unwrap();
            place(tree, rng, copy);
        }
        3 => {
            let el = tree
                .edit()
                .create_element(W, NAMES[rng.below(NAMES.len())])
                .unwrap();
            place(tree, rng, el);
        }
        _ => {
            let text = tree.edit().create_text("new").unwrap();
            place(tree, rng, text);
        }
    }
}

#[test]
fn sequences_of_edits_that_put_any_node_anywhere_always_scan_and_read() {
    let mut steps = 0;
    for seed in 1..=40u64 {
        let mut rng = Rng::new(seed);
        let mut g = Gen::new(seed);
        let mut tree = document(&g.body(4).xml);
        let mut prev = scan(&tree).unwrap();
        for _ in 0..25 {
            chaos(&mut tree, &mut rng);
            let story = scan(&tree).expect("a body that scanned once scans after any edit");
            // the edits are inside blocks too, so `touched` lists every block there was
            let touched: Vec<NodeId> = prev.blocks().iter().map(|e| e.node).collect();
            let refreshed = Story::refresh(&prev, &tree, &touched).unwrap();
            assert!(refreshed.story.same_as(&story), "seed {seed}");
            assert_eq!(
                refreshed.story.field_index(&tree),
                story.field_index(&tree),
                "seed {seed}"
            );
            assert_eq!(story.fingerprints(&tree).len(), story.blocks().len());
            for i in 0..story.blocks().len() {
                if let Some(p) = story.paragraph(&tree, i) {
                    check_laws(&tree, p.id(), &p.map(), &mut rng);
                }
            }
            let _ = (story.anchors(&tree), story.revisions(&tree));
            prev = story;
            steps += 1;
        }
    }
    assert_eq!(steps, 1000);
}

/// The wrappers above a block, outermost first, read the way the text model reads them.
fn chain(tree: &SpanTree, node: NodeId) -> Vec<(WrapperKind, NodeId)> {
    super::text_build::wrappers_above(&tree.node_ref(node))
}

/// How many seeded sequences the sweep runs. 20,000 were run once when the carried index was last
/// changed (500,000 edits, no difference); this many keep the debug build fast.
const SWEEP_SEEDS: u64 = 800;

#[test]
fn hundreds_of_chaos_sequences_keep_refresh_equal_to_a_scan_and_report_wrapper_changes() {
    // Proves: after every edit of 800 seeded sequences (25 edits each), `refresh` gives the blocks and
    // the field index of a scan, whether or not the previous story had built its index; and
    // `context_changed` is exactly the set of blocks in both stories whose chain of wrappers, read
    // by the text model's own function on the tree before and after, differs.
    let (mut shared, mut rebuilt, mut reported) = (0, 0, 0);
    for seed in 1..=SWEEP_SEEDS {
        let mut rng = Rng::new(seed);
        let mut g = Gen::new(seed);
        let mut tree = document(&g.body(4).xml);
        let mut prev = scan(&tree).unwrap();
        for _ in 0..25 {
            let before = tree.clone();
            if seed % 2 == 0 || rng.chance(50) {
                let _ = prev.fields(&before);
            }
            chaos(&mut tree, &mut rng);
            let story = scan(&tree).expect("a body that scanned once scans after any edit");
            let touched: Vec<NodeId> = prev.blocks().iter().map(|e| e.node).collect();
            let refreshed = Story::refresh(&prev, &tree, &touched).unwrap();
            assert!(refreshed.story.same_as(&story), "seed {seed}");
            assert_eq!(
                refreshed.story.field_index(&tree),
                story.field_index(&tree),
                "seed {seed}"
            );
            let expected: Vec<NodeId> = story
                .blocks()
                .iter()
                .map(|e| e.node)
                .filter(|&n| prev.index_of(n).is_some())
                .filter(|&n| chain(&before, n) != chain(&tree, n))
                .collect();
            assert_eq!(refreshed.context_changed, expected, "seed {seed}");
            reported += expected.len();
            if refreshed.fields_carried {
                shared += 1;
            } else {
                rebuilt += 1;
            }
            prev = story;
        }
    }
    assert!(reported > 50, "{reported} blocks changed wrappers");
    assert!(
        shared > 100 && rebuilt > 100,
        "{shared} shared, {rebuilt} rebuilt"
    );
}
