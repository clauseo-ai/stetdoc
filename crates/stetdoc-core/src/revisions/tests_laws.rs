//! The laws have teeth: a pair of documents that differ the way a bug would make them differ, and the law
//! that must say so. The soak shows that the compiler keeps the laws, and these show that the laws catch
//! what they are written for (the seeded bugs of the report are the same changes made in the compiler).

use super::laws_checks::existing_revisions_are_untouched;
use super::outcome::{Effect, Outcome};
use super::tests_support::*;

const ZED: &str = "w:author=\"Zed\" w:date=\"2026-01-01T00:00:00Z\"";

fn ins(id: u32, author_date: &str, text: &str) -> String {
    format!("<w:ins w:id=\"{id}\" {author_date}>{}</w:ins>", r(text))
}

fn with_effect(effect: Effect) -> Outcome {
    Outcome {
        effects: vec![effect],
        ..Outcome::default()
    }
}

#[test]
fn r4_sees_text_dropped_from_the_insertion_of_another_author() {
    let before = state(&p(&format!("{}{}", r("one "), ins(1, ZED, "kept text"))));
    let after = state(&p(&format!("{}{}", r("one "), ins(1, ZED, "kept"))));
    let err = existing_revisions_are_untouched(&before, &after, &Outcome::default()).unwrap_err();
    assert!(err.starts_with("R4 broken"), "{err}");
}

#[test]
fn r4_sees_the_insertion_of_another_author_with_another_author() {
    let before = state(&p(&ins(1, ZED, "text")));
    let after = state(&p(&ins(
        1,
        "w:author=\"Mallory\" w:date=\"2026-01-01T00:00:00Z\"",
        "text",
    )));
    let err = existing_revisions_are_untouched(&before, &after, &Outcome::default()).unwrap_err();
    assert!(err.starts_with("R4 broken"), "{err}");
}

#[test]
fn r4_sees_the_vanishing_of_a_deletion_and_of_what_it_held() {
    let del = |text: &str| {
        format!("<w:del w:id=\"1\" {ZED}><w:r><w:delText>{text}</w:delText></w:r></w:del>")
    };
    let before = state(&p(&format!("{}{}", r("a "), del("deleted words"))));
    let shortened = state(&p(&format!("{}{}", r("a "), del("deleted"))));
    let gone = state(&p(&r("a ")));
    for after in [&shortened, &gone] {
        let err =
            existing_revisions_are_untouched(&before, after, &Outcome::default()).unwrap_err();
        assert!(err.starts_with("R4 broken"), "{err}");
    }
}

#[test]
fn r4_lets_through_what_the_outcome_declares_and_the_halves_of_a_cut_revision() {
    let before = state(&p(&ins(1, ZED, "kept text")));
    let after = state(&p(&format!(
        "{}{}",
        ins(1, ZED, "kept"),
        ins(2, ZED, " text")
    )));
    // Cut in two, and the outcome says so by the old id: the old one holds less, and that is allowed.
    let cut = with_effect(Effect {
        split: vec![(1, 2)],
        ..Effect::default()
    });
    existing_revisions_are_untouched(&before, &after, &cut).unwrap();
    // Cut in two and not said: it is a change of what the revision holds.
    assert!(existing_revisions_are_untouched(&before, &after, &Outcome::default()).is_err());
    // An untouched pair is fine.
    existing_revisions_are_untouched(&before, &before, &Outcome::default()).unwrap();
}

// --- R6 and the range markers that stand between blocks

/// The body element and its paragraphs, in a transaction's main part.
fn body_and_paragraphs(
    tree: &crate::xml::SpanTree,
) -> (crate::wordml::NodeId, Vec<crate::wordml::NodeId>) {
    let body = tree
        .root()
        .child_elements()
        .find(|c| c.local_name() == "body")
        .unwrap();
    let paras = body
        .child_elements()
        .filter(|c| c.local_name() == "p")
        .map(|c| c.id())
        .collect();
    (body.id(), paras)
}

fn bookmark(tree: &mut crate::xml::SpanTree, id: &str) -> crate::wordml::NodeId {
    let uri = super::names::wns(tree);
    let m = super::tree_ops::create_w(tree, &uri, "bookmarkStart").unwrap();
    tree.edit().set_attr(m, &uri, "id", id).unwrap();
    tree.edit().set_attr(m, &uri, "name", "b").unwrap();
    m
}

fn r6(
    s: &crate::state::State,
    tx: &crate::state::Tx<'_>,
    outcome: &Outcome,
    homes: &super::laws_util::MarkerHomes,
) -> Result<(), String> {
    let mut cfg = super::laws_checks::Cfg::plain("Zed", "2026-01-01T00:00:00Z", 1);
    cfg.tracked = false;
    super::laws_checks::before_commit(s, s, tx, outcome, &cfg, homes)
}

fn homes_of(s: &crate::state::State) -> super::laws_util::MarkerHomes {
    let view = s.parts();
    super::laws_util::MarkerHomes::new(view.tree(view.id(MAIN).unwrap()).unwrap())
}

#[test]
fn r6_sees_a_new_marker_between_blocks_that_no_call_made_in_a_block() {
    let s = state(&format!("{}{}", p(&r("one")), p(&r("two"))));
    let homes = homes_of(&s);
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let (body, paras) = body_and_paragraphs(tree);
        let m = bookmark(tree, "7");
        super::tree_ops::insert(tree, body, Some(paras[1]), m).unwrap();
    }
    let err = r6(&s, &tx, &Outcome::default(), &homes).unwrap_err();
    assert!(err.contains("R6"), "{err}");
}

#[test]
fn r6_lets_through_a_marker_hoisted_out_of_a_block_that_went() {
    // A bookmark of the paragraph that went, hoisted between blocks.
    let s = state(&format!(
        "<w:p><w:bookmarkStart w:id=\"7\" w:name=\"b\"/>{}</w:p>{}",
        r("one"),
        p(&r("two"))
    ));
    let homes = homes_of(&s);
    let mut tx = s.begin();
    {
        let tree = tx.part_mut(MAIN).unwrap();
        let (_, paras) = body_and_paragraphs(tree);
        super::cleanup::remove_hoisting(tree, paras[0]).unwrap();
    }
    r6(&s, &tx, &Outcome::default(), &homes).unwrap();
    // A bookmark the batch made in a paragraph it made, and hoisted when that paragraph went: allowed when a
    // step saw it in that paragraph, and caught when none did.
    let s = state(&p(&r("one")));
    for seen in [true, false] {
        let mut homes = homes_of(&s);
        let mut tx = s.begin();
        let made = {
            let tree = tx.part_mut(MAIN).unwrap();
            let (body, paras) = body_and_paragraphs(tree);
            let uri = super::names::wns(tree);
            let made = super::tree_ops::create_w(tree, &uri, "p").unwrap();
            super::tree_ops::insert(tree, body, Some(paras[0]), made).unwrap();
            let m = bookmark(tree, "8");
            tree.edit().append_child(made, m).unwrap();
            if seen {
                homes.note(tree);
            }
            super::cleanup::remove_hoisting(tree, made).unwrap();
            made
        };
        let outcome = with_effect(Effect {
            new_blocks: vec![crate::wordml::BlockKey::new(MAIN.into(), made)],
            ..Effect::default()
        });
        assert_eq!(r6(&s, &tx, &outcome, &homes).is_ok(), seen, "seen: {seen}");
    }
}
