//! Memory probes: the peak resident size of an edit of a large body, which `std` cannot report, so
//! the tests are ignored and are run one at a time under the platform's own measurement. They use
//! no clock and no file, so they live in the core.
//!
//! ```text
//! cargo test --release -p stetdoc-core --no-run     # prints the test binary's path
//! /usr/bin/time -l <binary> state::tests_probe::dense_edit --ignored --exact
//! ```
//!
//! On macOS `-l` prints `maximum resident set size` in bytes, and on Linux `-v` prints it in
//! kilobytes. Run `*_open` for the cost of opening and `*_edit` for the whole path: open, 50 edits
//! with a story read after each, commit, drop the base, save and export. The peak includes building
//! the document, which is small beside the path itself. The budget of the task is 4 MiB + the file +
//! 8 times the XML of the parsed parts. `docs/design/refs-and-state.md` section 10.1 has the numbers
//! these were last run against, and `tests_corpus` has the documents' laws.

use super::tests::{package_bytes, para_xml};
use super::*;
use crate::xml::{NodeId, SpanTree};

/// A body of 15,935 paragraphs and 2,304 table rows in 21 tables, with `runs` runs in each
/// paragraph. With one long run each it is about 12 MB of XML and the arena costs 1.2 bytes per
/// byte. With eleven short runs it is 17 MB and the arena costs 4.0, which is denser than any
/// document in the corpora. The measured document with the most nodes, `bug65649`, costs 2.25.
fn synthetic_body(runs: usize) -> String {
    let text = if runs == 1 {
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor. ".repeat(6)
    } else {
        "word ".to_owned()
    };
    let run = |i: usize| {
        (0..runs)
            .map(|k| {
                format!(
                    "<w:r><w:rPr><w:b/><w:sz w:val=\"22\"/></w:rPr><w:t xml:space=\"preserve\">{i}x{k} {text}</w:t></w:r>"
                )
            })
            .collect::<String>()
    };
    let para = |i: usize| {
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"BodyText\"/><w:spacing w:before=\"120\" w:after=\"120\"/></w:pPr>{}</w:p>",
            run(i)
        )
    };
    let mut body = String::new();
    let mut n = 0;
    for _ in 0..13_631 {
        body.push_str(&para(n));
        n += 1;
    }
    for _ in 0..21 {
        body.push_str("<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"9000\"/></w:tblGrid>");
        for _ in 0..110 {
            body.push_str(&format!("<w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>", para(n)));
            n += 1;
        }
        body.push_str("</w:tbl>");
    }
    body.push_str(&para_xml("end"));
    body
}

/// The first text node below an element, if there is one.
fn first_text_below(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let r = tree.node_ref(n);
        if r.is_element() && r.local_name() == "t" {
            return r.first_child().map(|c| c.id());
        }
        let mark = stack.len();
        stack.extend(r.children().filter(|c| c.is_element()).map(|c| c.id()));
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    None
}

/// 50 operations that each change a paragraph and then read the story, which the write made stale,
/// and then one commit. Returns the new state.
fn batch(s: &State) -> State {
    let layout = s.parts().layout(false).unwrap();
    let blocks = &layout.stories[0].blocks;
    let main = layout.stories[0].part.to_string();
    let step = (blocks.len() / 50).max(1);
    let mut tx = s.begin();
    let id = tx.parts().id(&main).unwrap();
    for i in 0..50 {
        let Some(block) = blocks.get(i * step) else {
            break;
        };
        let tree = tx.part_mut(&main).unwrap();
        if let Some(t) = first_text_below(tree, block.key.node()) {
            tree.edit().set_text(t, "edited").unwrap();
        }
        tx.touch(&block.key);
        tx.parts().story(id).unwrap();
    }
    tx.commit().unwrap().0
}

fn open(runs: usize) -> State {
    State::open(package_bytes(&synthetic_body(runs)), &Limits::default()).unwrap()
}

fn edit(runs: usize) {
    let s = open(runs);
    let edited = batch(&s);
    drop(s);
    let saved = edited.to_package().unwrap().write().unwrap();
    let exported = edited.export().unwrap();
    assert!(!saved.is_empty() && !exported.head.is_empty());
}

#[test]
#[ignore = "memory probe: run under /usr/bin/time -l"]
fn light_open() {
    assert_eq!(open(1).parts().layout(false).unwrap().stories.len(), 1);
}

#[test]
#[ignore = "memory probe: run under /usr/bin/time -l"]
fn light_edit() {
    edit(1);
}

#[test]
#[ignore = "memory probe: run under /usr/bin/time -l"]
fn dense_open() {
    assert_eq!(open(11).parts().layout(false).unwrap().stories.len(), 1);
}

#[test]
#[ignore = "memory probe: run under /usr/bin/time -l"]
fn dense_edit() {
    edit(11);
}
