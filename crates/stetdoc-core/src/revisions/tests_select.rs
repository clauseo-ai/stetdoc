//! Known answers for the selectors of `resolve`: by id (the whole change), by element, by author, the
//! closure of a move, and the index that the compiler's calls keep up to date.

use super::index::ChangeIndex;
use super::resolve::resolve;
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::wordml::Mode;

fn rev(id: u32, author: &str) -> String {
    format!("w:id=\"{id}\" w:author=\"{author}\" w:date=\"2026-01-01T00:00:00Z\"")
}

fn ins(id: u32, author: &str, text: &str) -> String {
    format!("<w:ins {}>{}</w:ins>", rev(id, author), r(text))
}

fn run_step(body: &str, step: Step) -> String {
    let s = state(body);
    let mut tx = s.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &[step]).unwrap();
    body_xml(&tx)
}

fn step(action: Action, selector: Selector) -> Step {
    Step { action, selector }
}

#[test]
fn an_id_selects_the_whole_change_but_an_element_selects_one_element() {
    // Ann's insertion with Bob's inside it: ids 1 and 9 are one change of Ann's.
    let body = p(&format!(
        "{}{}{}",
        ins(1, "Ann", "x"),
        ins(2, "Bob", "new"),
        ins(9, "Ann", "y")
    ));
    let by_id = run_step(&body, step(Action::Reject, Selector::Ids(vec![9])));
    assert_eq!(by_id, p(&ins(2, "Bob", "new")));
    let by_element = run_step(&body, step(Action::Reject, Selector::Elements(vec![9])));
    assert_eq!(
        by_element,
        p(&format!("{}{}", ins(1, "Ann", "x"), ins(2, "Bob", "new")))
    );
}

#[test]
fn an_id_of_a_paragraph_mark_selects_the_enter_pair_and_a_reject_joins_the_halves() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p><w:p>{}{}</w:p>",
        rev(1, "Ann"),
        r("before"),
        ins(2, "Ann", "typed"),
        r(" after")
    );
    let s = state(&body);
    let mut tx = s.begin();
    let res = resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[step(Action::Reject, Selector::Ids(vec![1]))],
    )
    .unwrap();
    assert_eq!(texts(&tx, Mode::Final), ["before after"]);
    assert_eq!(res[0].resolved.len(), 2);
    assert_eq!(res[0].joined.len(), 1);
}

#[test]
fn the_other_side_of_a_move_is_selected_with_the_side_that_was() {
    // Ann moved the text, but Bob's author name is on the destination: selecting Ann takes both.
    let body = format!(
        "<w:p><w:moveFromRangeStart w:id=\"10\" w:name=\"m\"/><w:moveFrom {}>{}</w:moveFrom><w:moveFromRangeEnd w:id=\"10\"/>{}</w:p>\
         <w:p><w:moveToRangeStart w:id=\"11\" w:name=\"m\"/><w:moveTo {}>{}</w:moveTo><w:moveToRangeEnd w:id=\"11\"/></w:p>",
        rev(1, "Ann"),
        r("moved"),
        r(" stays"),
        rev(2, "Bob"),
        r("moved")
    );
    let s = state(&body);
    let mut tx = s.begin();
    let res = resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[step(Action::Accept, Selector::Author("Ann".into()))],
    )
    .unwrap();
    // Accepting a move removes the source and keeps the destination, so both sides were resolved.
    assert_eq!(res[0].resolved.len(), 2);
    assert_eq!(texts(&tx, Mode::Final), [" stays", "moved"]);
    assert!(!body_xml(&tx).contains("<w:move"));
}

#[test]
fn the_empty_author_selects_the_revisions_that_have_none() {
    let body = p(&format!(
        "<w:ins w:id=\"1\">{}</w:ins>{}",
        r("anonymous"),
        ins(2, "Ann", "named")
    ));
    let xml = run_step(&body, step(Action::Reject, Selector::Author(String::new())));
    assert_eq!(xml, p(&ins(2, "Ann", "named")));
}

#[test]
fn an_id_that_is_on_no_revision_is_not_found() {
    let s = state(&p(&ins(1, "Ann", "x")));
    let mut tx = s.begin();
    let err = resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[step(Action::Accept, Selector::Ids(vec![77]))],
    )
    .unwrap_err();
    assert_eq!(err.code(), crate::api::error::ErrorCode::NotFound);
}

#[test]
fn the_index_finds_what_a_call_wrote_and_forgets_what_a_call_removed() {
    let s = state(&p(&format!("{}{}", r("keep "), ins(1, "Clauseo", "mine"))));
    let mut tx = s.begin();
    let mut index = ChangeIndex::new();
    // The first lookup builds the index from the transaction's trees.
    assert_eq!(index.lookup(&tx, 1).unwrap().len(), 1);
    let ks = para_keys(&tx);
    let (fx, minted) = {
        let mut c = tracked(&mut tx);
        let fx = c.replace_text(&ks[0], 0..4, "KEEP").unwrap();
        index.note(c.tx(), &fx);
        (fx, c.tx().minted_annotation_ids())
    };
    let minted: Vec<u32> = minted.collect();
    assert!(minted.len() >= 2, "{minted:?}");
    for id in minted {
        assert_eq!(index.lookup(&tx, id as i64).unwrap().len(), 1, "id {id}");
    }
    // Removing the own insertion outright drops its id.
    let mut c = tracked(&mut tx);
    let fx2 = c.replace_text(&ks[0], 5..9, "").unwrap();
    index.note(c.tx(), &fx2);
    assert!(fx2.removed_own.iter().any(|x| x.id == Some(1)));
    assert!(index.lookup(&tx, 1).unwrap().is_empty());
    let _ = fx;
}

#[test]
fn the_index_finds_a_revision_that_a_call_writes_into_a_block_an_earlier_call_touched() {
    // The transaction lists a touched block once, so a call that writes into a block that is already
    // touched is told by the call's own list of blocks, and not by what is new in the transaction's.
    let s = state(&p(&r("one two three four")));
    let mut tx = s.begin();
    let mut index = ChangeIndex::new();
    let ks = para_keys(&tx);
    {
        let mut c = tracked(&mut tx);
        let fx = c.replace_text(&ks[0], 0..3, "1").unwrap();
        index.note(c.tx(), &fx);
        c.finish().unwrap();
    }
    // Builds the index; the paragraph is already in the touched list.
    let first: Vec<i64> = tx.minted_annotation_ids().map(i64::from).collect();
    assert_eq!(index.lookup(&tx, first[0]).unwrap().len(), 1);
    let mut c = tracked(&mut tx);
    let fx = c.replace_text(&ks[0], 6..9, "3").unwrap();
    index.note(c.tx(), &fx);
    let second: Vec<i64> = fx.minted.clone().map(i64::from).collect();
    assert!(!second.is_empty());
    drop(c);
    for id in second {
        assert_eq!(
            index.lookup(&tx, id).unwrap().len(),
            1,
            "the index misses id {id}, written into a block that was already touched"
        );
    }
}

#[test]
fn rejecting_what_a_call_just_wrote_by_its_id_in_the_same_transaction_restores_the_text() {
    let s = state(&p(&r("one two three")));
    let mut tx = s.begin();
    let mut index = ChangeIndex::new();
    let ks = para_keys(&tx);
    let mut c = tracked(&mut tx);
    let fx = c.replace_text(&ks[0], 4..7, "2").unwrap();
    index.note(c.tx(), &fx);
    let ids: Vec<i64> = c.tx().minted_annotation_ids().map(i64::from).collect();
    // The index was never built, so the lookup builds it from the staged trees, which hold the new ids.
    resolve(
        &mut tx,
        &mut index,
        &[step(Action::Reject, Selector::Ids(ids))],
    )
    .unwrap();
    assert_eq!(texts(&tx, Mode::Final), ["one two three"]);
    assert!(!body_xml(&tx).contains("<w:ins ") && !body_xml(&tx).contains("<w:del "));
}

/// Two moved paragraphs, one range start before the first wrapper and the end after the second, and the
/// destination as one paragraph.
fn two_paragraph_move() -> String {
    format!(
        "<w:p><w:moveFromRangeStart w:id=\"10\" w:name=\"m\"/><w:moveFrom {}>{}</w:moveFrom></w:p>\
         <w:p><w:moveFrom {}>{}</w:moveFrom><w:moveFromRangeEnd w:id=\"10\"/></w:p>\
         <w:p>{}<w:moveToRangeStart w:id=\"11\" w:name=\"m\"/><w:moveTo {}>{}</w:moveTo><w:moveToRangeEnd w:id=\"11\"/></w:p>",
        rev(1, "Ann"),
        r("one "),
        rev(2, "Ann"),
        r("two"),
        r("kept "),
        rev(3, "Ann"),
        r("one two")
    )
}

#[test]
fn a_move_over_two_paragraphs_is_one_change_with_the_destination() {
    let s = state(&two_paragraph_move());
    let view = s.parts();
    let id = view.id(MAIN).unwrap();
    let cs = super::changes::changes(&view.story(id).unwrap(), view.tree(id).unwrap(), None);
    assert_eq!(cs.len(), 1, "{cs:?}");
    assert_eq!(cs[0].ids, [1, 2, 3]);
}

#[test]
fn resolving_a_move_by_the_id_of_one_wrapper_resolves_all_of_it_and_drops_its_range_markers() {
    for (action, kept) in [
        (Action::Accept, ["", "", "kept one two"]),
        (Action::Reject, ["one ", "two", "kept "]),
    ] {
        let s = state(&two_paragraph_move());
        let mut tx = s.begin();
        resolve(
            &mut tx,
            &mut ChangeIndex::new(),
            &[step(action, Selector::Ids(vec![2]))],
        )
        .unwrap();
        let xml = body_xml(&tx);
        assert!(!xml.contains("<w:move"), "{action:?}: {xml}");
        assert_eq!(texts(&tx, Mode::Final), kept, "{action:?}");
    }
}
