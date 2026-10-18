//! Origins (V6): a unit that receives content copied from another unit may keep the problems the
//! other unit had, if the edit says where the content came from. Without the declaration it is
//! refused exactly when the origin had a problem, which is the safe direction.

use super::keys::Key;
use super::tests::state;
use super::tests_edit::{
    MAIN, Said, accepted, append, base_node, el, insert_first, key, nth, refused, run_of, say,
    set_attr, transaction_with,
};
use super::{Declared, Origin, edit, unit_keys};
use crate::api::check::FindingCode as C;
use crate::api::error::ErrorCode;
use crate::state::State;
use crate::wordml::BlockKey;
use crate::xml::{NodeId, SpanTree};
use std::sync::Arc;

/// A paragraph whose only run has an empty properties element.
const FAULTY: &str = "<w:p><w:r><w:rPr/><w:t>a</w:t></w:r></w:p>";
const CLEAN: &str = "<w:p><w:r><w:t>a</w:t></w:r></w:p>";

fn declare(block: NodeId, from: &[NodeId]) -> Declared {
    Declared {
        origins: vec![Origin {
            block: key(block),
            from: from.iter().copied().map(key).collect(),
        }],
        foreign: Vec::new(),
        parts: Vec::new(),
    }
}

/// Puts `new` into the body before the section properties.
fn place(t: &mut SpanTree, new: NodeId) {
    let body = nth(t, "body", 0);
    let sect = nth(t, "sectPr", 0);
    t.edit()
        .insert_before(body, Some(sect), new)
        .expect("inserts");
}

/// A copy of the first paragraph, put after it.
fn copy_of_the_first(t: &mut SpanTree) -> (NodeId, NodeId) {
    let p = nth(t, "p", 0);
    let copy = t.edit().clone_subtree(p).expect("clones");
    place(t, copy);
    (p, copy)
}

fn judge(s: &State, build: impl FnOnce(&mut SpanTree) -> Declared) -> (Said, Said) {
    // the same edit, with its declaration and without it
    let (tx, declared) = transaction_with(s, build);
    let with = say(&tx, &declared);
    let without = say(&tx, &Declared::default());
    (with, without)
}

#[test]
fn a_copy_may_keep_the_problem_of_the_paragraph_it_was_copied_from() {
    let s = state(FAULTY);
    let (with, without) = judge(&s, |t| {
        let (p, copy) = copy_of_the_first(t);
        declare(copy, &[p])
    });
    assert!(accepted(&with).carried_total >= 1);
    refused(&without);
    assert_eq!(without.new, [C::EmptyProperties]);
    // a copy of a paragraph with no problem has nothing to excuse
    let s = state(CLEAN);
    let (with, without) = judge(&s, |t| {
        let (p, copy) = copy_of_the_first(t);
        declare(copy, &[p])
    });
    accepted(&with);
    accepted(&without);
}

#[test]
fn the_second_half_of_a_split_takes_the_problem_of_the_paragraph_it_came_from() {
    let s = state("<w:p><w:r><w:t>a</w:t></w:r><w:r><w:rPr/><w:t>b</w:t></w:r></w:p>");
    let (with, without) = judge(&s, |t| {
        let p = nth(t, "p", 0);
        let second = el(t, "p");
        let r = nth(t, "r", 1);
        t.edit().detach(r).expect("detaches");
        append(t, second, r);
        place(t, second);
        declare(second, &[p])
    });
    // the first half keeps the element and so its own origin: the paragraph before the edit
    accepted(&with);
    refused(&without);
    assert_eq!(without.new, [C::EmptyProperties]);
}

#[test]
fn the_survivor_of_a_merge_keeps_the_problems_of_both_paragraphs() {
    let body = "<w:p><w:r><w:rPr/><w:t>a</w:t></w:r></w:p><w:p><w:r><w:t xml:space=\"default\"> b</w:t></w:r></w:p>";
    let s = state(body);
    let merge = |t: &mut SpanTree| {
        let (first, second) = (nth(t, "p", 0), nth(t, "p", 1));
        let r = nth(t, "r", 1);
        t.edit().detach(r).expect("detaches");
        append(t, first, r);
        t.edit().detach(second).expect("detaches");
        (first, second)
    };
    let (with, without) = judge(&s, |t| {
        let (first, second) = merge(t);
        declare(first, &[second])
    });
    // the survivor has its own problem, and the second paragraph's, which came with its run: the second paragraph is gone and
    // its run stands in the survivor, so the join carries its keys whether or not the compiler declared it
    accepted(&with);
    accepted(&without);
    // a run moved out of a paragraph that is still there brings nothing without a declaration
    let (_, kept) = judge(&s, |t| {
        let first = nth(t, "p", 0);
        let r = nth(t, "r", 1);
        t.edit().detach(r).expect("detaches");
        append(t, first, r);
        Declared::default()
    });
    refused(&kept);
    assert_eq!(kept.new, [C::EdgeSpaceWithoutPreserve]);
    // two paragraphs with the same kind of problem need no declaration
    let same = state(
        "<w:p><w:r><w:rPr/><w:t>a</w:t></w:r></w:p><w:p><w:r><w:rPr/><w:t>b</w:t></w:r></w:p>",
    );
    let (_, without) = judge(&same, |t| {
        let (first, second) = merge(t);
        declare(first, &[second])
    });
    accepted(&without);
}

#[test]
fn each_paragraph_of_a_cloned_row_declares_the_paragraph_it_copies() {
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1\"/></w:tblGrid><w:tr><w:tc>{FAULTY}</w:tc></w:tr></w:tbl>"
    );
    let s = state(&table);
    let (with, without) = judge(&s, |t| {
        let (tbl, row, p) = (nth(t, "tbl", 0), nth(t, "tr", 0), nth(t, "p", 0));
        let copy = t.edit().clone_subtree(row).expect("clones");
        append(t, tbl, copy);
        // the paragraph of the copy is the second paragraph of the document
        let new = named_p(t, 1);
        declare(new, &[p])
    });
    accepted(&with);
    refused(&without);
    assert_eq!(without.new, [C::EmptyProperties]);
}

fn named_p(t: &SpanTree, n: usize) -> NodeId {
    nth(t, "p", n)
}

#[test]
fn a_row_stands_for_the_table_that_holds_it() {
    // a table with no grid already breaks the schema; a copy of a row is no new violation
    let table = format!("<w:tbl><w:tblPr/><w:tr><w:tc>{CLEAN}</w:tc></w:tr></w:tbl>");
    let s = state(&table);
    let (with, without) = judge(&s, |t| {
        let (tbl, row) = (nth(t, "tbl", 0), nth(t, "tr", 0));
        let copy = t.edit().clone_subtree(row).expect("clones");
        append(t, tbl, copy);
        declare(copy, &[row])
    });
    assert!(accepted(&with).carried_total >= 1);
    assert!(accepted(&without).carried_total >= 1);
}

#[test]
fn an_origin_can_be_made_from_another_that_the_same_edit_declared() {
    let s = state(FAULTY);
    let (tx, declared) = transaction_with(&s, |t| {
        let p = nth(t, "p", 0);
        // the second paragraph is a copy of the first, the third a copy of the second
        let second = t.edit().clone_subtree(p).expect("clones");
        place(t, second);
        let third = t.edit().clone_subtree(second).expect("clones");
        place(t, third);
        Declared {
            origins: vec![
                Origin {
                    block: key(second),
                    from: vec![key(p)],
                },
                Origin {
                    block: key(third),
                    from: vec![key(second)],
                },
            ],
            foreign: Vec::new(),
            parts: Vec::new(),
        }
    });
    assert!(accepted(&say(&tx, &declared)).carried_total >= 2);
}

#[test]
fn an_origin_that_names_nothing_the_base_has_is_a_bug_of_the_caller() {
    let s = state(FAULTY);
    let (tx, declared) = transaction_with(&s, |t| {
        let p = nth(t, "p", 0);
        // a block that exists only in the transaction, and was made from nothing declared
        let stray = el(t, "p");
        let r = run_of(t, "x");
        append(t, stray, r);
        place(t, stray);
        let text = t.node_ref(nth(t, "t", 0)).first_child().map(|c| c.id());
        t.edit()
            .set_text(text.expect("text"), "changed")
            .expect("sets");
        declare(p, &[stray])
    });
    let e = edit(&tx, &declared).expect_err("refused");
    assert_eq!(e.code(), ErrorCode::Internal, "{}", e.message());
    // a block of a part that has no schema is the same
    let bad = Declared {
        origins: vec![Origin {
            block: BlockKey::new(Arc::from("word/media/image1.png"), base_node(&s, "p", 0)),
            from: Vec::new(),
        }],
        foreign: Vec::new(),
        parts: Vec::new(),
    };
    let e = edit(&tx, &bad).expect_err("refused");
    assert_eq!(e.code(), ErrorCode::Internal, "{}", e.message());
}

#[test]
fn a_block_from_another_package_brings_the_keys_it_had_there() {
    // the original, in which a paragraph had the problem
    let other = state(FAULTY);
    let view = other.parts();
    let id = view.id(MAIN).expect("main");
    let theirs = key(nth(view.tree(id).expect("parses"), "p", 0));
    let keys = unit_keys(&view, &theirs).expect("keys");
    assert!(
        keys.contains(&Key::rule(C::EmptyProperties, "rPr")),
        "{keys:?}"
    );
    // the redline imports a paragraph with the same problem as a new block
    let s = state(CLEAN);
    let import = |t: &mut SpanTree| {
        let p = el(t, "p");
        let run = run_of(t, "imported");
        let rpr = el(t, "rPr");
        insert_first(t, run, rpr);
        append(t, p, run);
        place(t, p);
        p
    };
    let (tx, p) = transaction_with(&s, import);
    let foreign = Declared {
        origins: Vec::new(),
        foreign: vec![(key(p), keys)],
        parts: Vec::new(),
    };
    accepted(&say(&tx, &foreign));
    refused(&say(&tx, &Declared::default()));
    // a block that is not in the document is the caller's mistake
    let e = unit_keys(&view, &key(nth_new(&view))).expect_err("no such element");
    assert_eq!(e.code(), ErrorCode::Internal);
}

/// A node id that no part of this view has.
fn nth_new(view: &crate::state::PartsView<'_>) -> NodeId {
    let id = view.id(MAIN).expect("main");
    let mut tree = view.tree(id).expect("parses").clone();
    tree.edit()
        .create_element(super::tests::W, "p")
        .expect("creates")
}

#[test]
fn a_declaration_never_hides_a_problem_in_a_unit_it_does_not_name() {
    // the second paragraph has no problem, and a change that gives it one is refused although
    // the first paragraph has that problem and the edit declares the first as an origin of the
    // third
    let s = state(&format!("{FAULTY}{CLEAN}{CLEAN}"));
    let (tx, declared) = transaction_with(&s, |t| {
        let (first, second, third) = (nth(t, "p", 0), nth(t, "p", 1), nth(t, "p", 2));
        let rpr = el(t, "rPr");
        let r = nth(t, "r", 1);
        let at = t.node_ref(r).first_child().map(|c| c.id());
        t.edit().insert_before(r, at, rpr).expect("inserts");
        set_attr(t, third, "rsidR", "00112233");
        declare(third, &[first, second])
    });
    let said = say(&tx, &declared);
    refused(&said);
    assert_eq!(said.new, [C::EmptyProperties]);
}
