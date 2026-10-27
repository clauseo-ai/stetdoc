//! `check::edit` on small documents: the laws V5 to V9 of `docs/design/check.md` 9. The edits are
//! made through the span tree's `Editor` on a `Tx`, as the compiler makes them, and no test calls
//! `Tx::touch`: the units are found from the tree and from nothing that is declared (V7).

use super::edit::{Mode, collect, run};
use super::tests::{W, named, state, state_with};
use super::{Declared, Verified, edit};
use crate::api::check::{Class, FindingCode as C};
use crate::api::error::{ErrorCode, StetError};
use crate::state::{State, Tx};
use crate::wordml::BlockKey;
use crate::xml::{NodeId, SpanTree};
use std::sync::Arc;

pub(super) const MAIN: &str = "word/document.xml";

/// A transaction on the state whose main part `change` has edited.
pub(super) fn transaction<'s>(state: &'s State, change: impl FnOnce(&mut SpanTree)) -> Tx<'s> {
    let mut tx = state.begin();
    change(tx.part_mut(MAIN).expect("the main part stages"));
    tx
}

/// A transaction that edits another part of the document.
pub(super) fn in_part<'s>(s: &'s State, name: &str, change: impl FnOnce(&mut SpanTree)) -> Tx<'s> {
    let mut tx = s.begin();
    change(tx.part_mut(name).expect("the part stages"));
    tx
}

/// A transaction whose main part `change` has edited, and what `change` returned.
pub(super) fn transaction_with<'s, R>(
    state: &'s State,
    change: impl FnOnce(&mut SpanTree) -> R,
) -> (Tx<'s>, R) {
    let mut tx = state.begin();
    let out = change(tx.part_mut(MAIN).expect("the main part stages"));
    (tx, out)
}

/// The nth element with this local name in the main part of a state.
pub(super) fn base_node(state: &State, local: &str, n: usize) -> NodeId {
    let view = state.parts();
    let id = view.id(MAIN).expect("the main part");
    nth(view.tree(id).expect("parses"), local, n)
}

/// The key of a block of the main part.
pub(super) fn key(node: NodeId) -> BlockKey {
    BlockKey::new(Arc::from(MAIN), node)
}

/// The nth element with this local name.
pub(super) fn nth(tree: &SpanTree, local: &str, n: usize) -> NodeId {
    named(tree, local)
        .get(n)
        .copied()
        .unwrap_or_else(|| panic!("no w:{local} number {n}"))
}

/// A new, detached Word element.
pub(super) fn el(tree: &mut SpanTree, local: &str) -> NodeId {
    tree.edit().create_element(W, local).expect("creates")
}

/// A new, detached Word element with attributes.
pub(super) fn el_with(tree: &mut SpanTree, local: &str, attrs: &[(&str, &str)]) -> NodeId {
    let id = el(tree, local);
    for (name, value) in attrs {
        tree.edit().set_attr(id, W, name, value).expect("sets");
    }
    id
}

/// A run with this text, detached.
pub(super) fn run_of(tree: &mut SpanTree, text: &str) -> NodeId {
    let r = el(tree, "r");
    let t = el(tree, "t");
    let text = tree.edit().create_text(text).expect("creates");
    let mut ed = tree.edit();
    ed.append_child(t, text).expect("attaches");
    ed.append_child(r, t).expect("attaches");
    r
}

/// Sets the text of the nth `w:t`.
pub(super) fn retext(t: &mut SpanTree, n: usize, text: &str) {
    let node = t.node_ref(nth(t, "t", n)).first_child().map(|c| c.id());
    t.edit()
        .set_text(node.expect("a text node"), text)
        .expect("sets the text");
}

/// Run properties out of canonical order, as XML: `w:i` before `w:b`. A schema violation, where an
/// empty properties element is only a note.
pub(super) const MISORDERED_RPR: &str = "<w:rPr><w:i/><w:b/></w:rPr>";

/// A new, detached `w:rPr` that is `MISORDERED_RPR`.
pub(super) fn misordered_rpr(tree: &mut SpanTree) -> NodeId {
    let rpr = el(tree, "rPr");
    let i = el(tree, "i");
    let b = el(tree, "b");
    append(tree, rpr, i);
    append(tree, rpr, b);
    rpr
}

pub(super) fn append(tree: &mut SpanTree, parent: NodeId, child: NodeId) {
    tree.edit().append_child(parent, child).expect("attaches");
}

pub(super) fn insert_first(tree: &mut SpanTree, parent: NodeId, child: NodeId) {
    let first = tree.node_ref(parent).first_child().map(|c| c.id());
    tree.edit()
        .insert_before(parent, first, child)
        .expect("attaches");
}

pub(super) fn detach(tree: &mut SpanTree, node: NodeId) {
    tree.edit().detach(node).expect("detaches");
}

pub(super) fn set_attr(tree: &mut SpanTree, node: NodeId, name: &str, value: &str) {
    tree.edit().set_attr(node, W, name, value).expect("sets");
}

/// What `check::edit` says about an edit, with the shortcuts and without them.
pub(super) struct Said {
    pub(super) result: Result<Verified, StetError>,
    pub(super) new: Vec<C>,
}

/// Judges an edit twice, with the dirty flags and the equality test and then with every unit of
/// every changed part, and fails if the two disagree (V8).
pub(super) fn say(tx: &Tx<'_>, declared: &Declared) -> Said {
    let fast = run(tx, declared, Mode::Shortcuts);
    let slow = run(tx, declared, Mode::Everything);
    assert_eq!(
        fast.is_ok(),
        slow.is_ok(),
        "the shortcuts and the full check disagree: {:?} against {:?}",
        fast.as_ref().err().map(StetError::message),
        slow.as_ref().err().map(StetError::message)
    );
    let (_, quick) = collect(tx, declared, Mode::Shortcuts).expect("collects");
    let (_, full) = collect(tx, declared, Mode::Everything).expect("collects");
    assert_eq!(quick.new_codes(), full.new_codes());
    assert_eq!(quick.new_total(), full.new_total());
    Said {
        result: fast,
        new: quick.new_codes(),
    }
}

pub(super) fn said(state: &State, change: impl FnOnce(&mut SpanTree)) -> Said {
    say(&transaction(state, change), &Declared::default())
}

/// A refusal is `validation_failed` and says how many violations it adds.
pub(super) fn refused(said: &Said) -> &str {
    let e = said.result.as_ref().expect_err("the edit is refused");
    assert_eq!(e.code(), ErrorCode::ValidationFailed, "{}", e.message());
    assert!(
        e.message().starts_with("The edit would add "),
        "{}",
        e.message()
    );
    e.message()
}

pub(super) fn accepted(said: &Said) -> &Verified {
    said.result
        .as_ref()
        .unwrap_or_else(|e| panic!("the edit is accepted, and was refused: {}", e.message()))
}

/// A change to the main part, and the findings it adds to a unit that had none.
struct Case {
    name: &'static str,
    clean: &'static str,
    /// The same unit, with a problem that has the key the change adds.
    already: &'static str,
    change: fn(&mut SpanTree),
    codes: &'static [C],
}

fn second_ppr(t: &mut SpanTree) {
    let p = nth(t, "p", 0);
    let ppr = el(t, "pPr");
    let jc = el_with(t, "jc", &[("val", "left")]);
    append(t, ppr, jc);
    append(t, p, ppr);
}

fn bogus_attribute(t: &mut SpanTree) {
    let r = nth(t, "r", 1);
    set_attr(t, r, "bogus", "1");
}

fn bold_maybe(t: &mut SpanTree) {
    let b = nth(t, "b", 1);
    set_attr(t, b, "val", "maybe");
}

fn text_in_the_deletion(t: &mut SpanTree) {
    let del = nth(t, "del", 0);
    let r = run_of(t, "y");
    append(t, del, r);
}

fn deleted_text_outside(t: &mut SpanTree) {
    let p = nth(t, "p", 0);
    let r = el(t, "r");
    let dt = el(t, "delText");
    let text = t.edit().create_text("y").expect("creates");
    append(t, dt, text);
    append(t, r, dt);
    append(t, p, r);
}

fn edge_space(t: &mut SpanTree) {
    let text = t.node_ref(nth(t, "t", 1)).first_child().map(|c| c.id());
    t.edit()
        .set_text(text.expect("text"), " b")
        .expect("sets the text");
}

fn nested_ins(t: &mut SpanTree) {
    let ins = nth(t, "ins", 0);
    let inner = el_with(
        t,
        "ins",
        &[
            ("id", "9"),
            ("author", "a"),
            ("date", "2024-01-01T00:00:00Z"),
        ],
    );
    let r = run_of(t, "n");
    append(t, inner, r);
    append(t, ins, inner);
}

fn empty_ins(t: &mut SpanTree) {
    let p = nth(t, "p", 0);
    let ins = el_with(
        t,
        "ins",
        &[
            ("id", "9"),
            ("author", "a"),
            ("date", "2024-01-01T00:00:00Z"),
        ],
    );
    append(t, p, ins);
}

fn misordered_rpr_in_second_run(t: &mut SpanTree) {
    let r = nth(t, "r", 1);
    let rpr = misordered_rpr(t);
    insert_first(t, r, rpr);
}

fn repeat_the_id(t: &mut SpanTree) {
    let last = named(t, "ins").len() - 1;
    let ins = nth(t, "ins", last);
    set_attr(t, ins, "id", "1");
}

fn repeat_the_bookmark_id(t: &mut SpanTree) {
    for local in ["bookmarkStart", "bookmarkEnd"] {
        let m = nth(t, local, 1);
        set_attr(t, m, "id", "1");
    }
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "a second paragraph properties element, after the runs",
            clean: r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p>"#,
            already: r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t>a</w:t></w:r><w:pPr><w:jc w:val="right"/></w:pPr></w:p>"#,
            change: second_ppr,
            codes: &[C::SchemaOutOfOrder],
        },
        Case {
            name: "an attribute the schema does not define",
            clean: r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#,
            already: r#"<w:p><w:r w:bogus="1"><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#,
            change: bogus_attribute,
            codes: &[C::SchemaUnknownAttribute],
        },
        Case {
            name: "a value that does not satisfy its type",
            clean: r#"<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>a</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>b</w:t></w:r></w:p>"#,
            already: r#"<w:p><w:r><w:rPr><w:b w:val="maybe"/></w:rPr><w:t>a</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>b</w:t></w:r></w:p>"#,
            change: bold_maybe,
            codes: &[C::SchemaInvalidAttributeValue],
        },
        Case {
            name: "text in a deletion",
            clean: r#"<w:p><w:del w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:delText>x</w:delText></w:r></w:del></w:p>"#,
            already: r#"<w:p><w:del w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>x</w:t></w:r></w:del></w:p>"#,
            change: text_in_the_deletion,
            codes: &[C::TextInDeletion],
        },
        Case {
            name: "deleted text outside a deletion",
            clean: r#"<w:p><w:r><w:t>a</w:t></w:r></w:p>"#,
            already: r#"<w:p><w:r><w:delText>a</w:delText></w:r></w:p>"#,
            change: deleted_text_outside,
            codes: &[C::DeltextOutsideDeletion],
        },
        Case {
            name: "white space at the edge of text with no xml:space",
            clean: r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#,
            already: r#"<w:p><w:r><w:t> a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#,
            change: edge_space,
            codes: &[C::EdgeSpaceWithoutPreserve],
        },
        Case {
            name: "an insertion inside an insertion",
            clean: r#"<w:p><w:ins w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>a</w:t></w:r></w:ins></w:p>"#,
            already: r#"<w:p><w:ins w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:ins w:id="2" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>a</w:t></w:r></w:ins></w:ins></w:p>"#,
            change: nested_ins,
            codes: &[C::RevisionNestedInItself],
        },
        Case {
            name: "an empty insertion",
            clean: r#"<w:p><w:r><w:t>a</w:t></w:r></w:p>"#,
            already: r#"<w:p><w:r><w:t>a</w:t></w:r><w:ins w:id="2" w:author="a" w:date="2024-01-01T00:00:00Z"/></w:p>"#,
            change: empty_ins,
            codes: &[C::EmptyRevision],
        },
        Case {
            name: "a revision id used twice",
            clean: r#"<w:p><w:ins w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>a</w:t></w:r></w:ins></w:p><w:p><w:ins w:id="2" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>b</w:t></w:r></w:ins></w:p>"#,
            already: r#"<w:p><w:ins w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>a</w:t></w:r></w:ins></w:p><w:p><w:ins w:id="1" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>b</w:t></w:r></w:ins></w:p><w:p><w:ins w:id="3" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>c</w:t></w:r></w:ins></w:p>"#,
            change: repeat_the_id,
            codes: &[C::RevisionIdDuplicate],
        },
        Case {
            name: "a bookmark id used twice",
            clean: r#"<w:p><w:bookmarkStart w:id="1" w:name="a"/><w:r><w:t>a</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:bookmarkStart w:id="2" w:name="b"/><w:r><w:t>b</w:t></w:r><w:bookmarkEnd w:id="2"/></w:p>"#,
            already: r#"<w:p><w:bookmarkStart w:id="1" w:name="a"/><w:r><w:t>a</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:bookmarkStart w:id="1" w:name="b"/><w:r><w:t>b</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:bookmarkStart w:id="2" w:name="c"/><w:bookmarkEnd w:id="2"/></w:p>"#,
            change: repeat_the_bookmark_id,
            codes: &[C::BookmarkIdDuplicate],
        },
    ]
}

#[test]
fn v5_a_change_that_adds_a_finding_to_a_clean_unit_is_refused_and_one_that_adds_a_key_it_had_is_not()
 {
    for case in cases() {
        let clean = state(case.clean);
        let first = said(&clean, case.change);
        let message = refused(&first).to_owned();
        assert_eq!(first.new, case.codes, "{}: {message}", case.name);
        let already = state(case.already);
        let again = said(&already, case.change);
        let verified = accepted(&again);
        assert!(
            verified.carried_total >= 1,
            "{}: nothing was carried",
            case.name
        );
    }
}

#[test]
fn a_run_property_out_of_order_is_counted_so_another_copy_is_new_even_where_the_unit_had_one() {
    // The Open XML SDK counts each out-of-order w:rPr as a schema error, and the compiler writes every container it creates in
    // order, so one more than the origin had is new, whether or not the origin had the key.
    let clean = said(
        &state(r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#),
        misordered_rpr_in_second_run,
    );
    refused(&clean);
    assert_eq!(clean.new, [C::SchemaNonCanonicalOrder]);
    let one =
        r#"<w:p><w:r><w:rPr><w:i/><w:b/></w:rPr><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#;
    let another = said(&state(one), misordered_rpr_in_second_run);
    refused(&another);
    assert_eq!(another.new, [C::SchemaNonCanonicalOrder]);
    // An edit that keeps the count carries the one that was there.
    let kept = said(&state(one), |t| retext(t, 1, "edited"));
    assert!(accepted(&kept).carried_total >= 1);
}

#[test]
fn an_empty_property_container_is_a_note_that_blocks_nothing() {
    // the schema allows an empty w:rPr, so an edit that adds one is accepted, and the note is
    // reported in the verification, not counted as a problem
    let s = state(r#"<w:p><w:r><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r></w:p>"#);
    let added = said(&s, |t| {
        let r = nth(t, "r", 1);
        let rpr = el(t, "rPr");
        insert_first(t, r, rpr);
    });
    let v = accepted(&added);
    assert!(added.new.is_empty(), "{:?}", added.new);
    assert_eq!(v.carried_total, 0);
    assert_eq!(v.notes.len(), 1, "{:?}", v.notes);
    assert_eq!(v.notes[0].code, C::EmptyProperties);
    assert_eq!(v.notes[0].class, Class::Note);
    // a note is not carried: a paragraph that already has one, and is edited, brings nothing
    let s = state(r#"<w:p><w:r><w:rPr/><w:t>a</w:t></w:r></w:p>"#);
    let edited = said(&s, |t| retext(t, 0, "edited"));
    let v = accepted(&edited);
    assert_eq!(v.carried_total, 0, "{:?}", v.carried);
    assert!(v.carried.is_empty() && v.notes.is_empty(), "{v:?}");
}

#[test]
fn the_refusal_names_the_first_findings_with_their_refs() {
    let s = state(
        r#"<w:p><w:r><w:t>a</w:t></w:r></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p><w:p><w:r><w:t>c</w:t></w:r></w:p>"#,
    );
    let tx = transaction(&s, |t| {
        for n in 0..3 {
            let r = nth(t, "r", n);
            let rpr = misordered_rpr(t);
            insert_first(t, r, rpr);
        }
        let p = nth(t, "p", 2);
        let ppr = el(t, "pPr");
        let jc = el_with(t, "jc", &[("val", "left")]);
        append(t, ppr, jc);
        append(t, p, ppr);
    });
    let e = edit(&tx, &Declared::default()).expect_err("refused");
    assert_eq!(e.code(), ErrorCode::ValidationFailed);
    let m = e.message();
    assert!(
        m.starts_with("The edit would add 4 violations. p1: "),
        "{m}"
    );
    assert!(m.contains(" p2: ") && m.contains(" p3: "), "{m}");
    assert!(m.ends_with(" And 1 more."), "{m}");
}

/// Deletes the first run the way the compiler does: the run goes into a `w:del`, and its `w:t`
/// is renamed `w:delText`, which is a new element with the old one's attributes and text.
fn delete_the_first_run(t: &mut SpanTree) {
    let (p, r) = (nth(t, "p", 0), nth(t, "r", 0));
    let del = el_with(
        t,
        "del",
        &[
            ("id", "1"),
            ("author", "a"),
            ("date", "2024-01-01T00:00:00Z"),
        ],
    );
    let old = nth(t, "t", 0);
    let renamed = el(t, "delText");
    for (name, value) in [("space", "preserve"), ("bogus", "1")] {
        if t.node_ref(old).attr(W, name).is_some() {
            set_attr(t, renamed, name, value);
        }
    }
    let text = t.node_ref(old).first_child().map(|c| c.id());
    if let Some(text) = text {
        detach(t, text);
        append(t, renamed, text);
    }
    detach(t, old);
    append(t, r, renamed);
    let first = t
        .node_ref(r)
        .parent()
        .and_then(|x| x.first_child())
        .map(|c| c.id());
    t.edit().insert_before(p, first, del).expect("inserts");
    detach(t, r);
    append(t, del, r);
}

#[test]
fn a_problem_that_an_element_had_keeps_its_key_when_the_compiler_renames_the_element() {
    // python-docx writes w:space on w:t, which the schema does not define
    let s = state(r#"<w:p><w:r><w:t w:space="preserve">a</w:t></w:r></w:p>"#);
    let renamed = said(&s, delete_the_first_run);
    let v = accepted(&renamed);
    assert!(
        v.carried_total >= 1,
        "the unknown attribute is carried: {:?}",
        v.carried
    );
    // an element that had no problem does not get one by being renamed
    let clean = state(r#"<w:p><w:r><w:t>a</w:t></w:r></w:p>"#);
    accepted(&said(&clean, delete_the_first_run));
    // white space at the edge of the text is the same problem under the new name
    let s = state(r#"<w:p><w:r><w:t> a</w:t></w:r></w:p>"#);
    accepted(&said(&s, delete_the_first_run));
    // and a new problem on the renamed element is new
    let s = state(r#"<w:p><w:r><w:t>a</w:t></w:r></w:p>"#);
    let said = said(&s, |t| {
        delete_the_first_run(t);
        let dt = nth(t, "delText", 0);
        set_attr(t, dt, "bogus", "1");
    });
    refused(&said);
    assert_eq!(said.new, [C::SchemaUnknownAttribute]);
}

const REV: &str = r#"w:author="a" w:date="2024-01-01T00:00:00Z""#;

#[test]
fn what_the_census_finds_after_the_edit_and_not_before_is_refused() {
    // a bookmark loses its end
    let s = state(
        r#"<w:p><w:bookmarkStart w:id="1" w:name="a"/><w:r><w:t>a</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p>"#,
    );
    let lost = said(&s, |t| {
        let end = nth(t, "bookmarkEnd", 0);
        detach(t, end);
    });
    refused(&lost);
    assert_eq!(lost.new, [C::BookmarkUnpaired]);
    // a bookmark that was unpaired in the input is carried by an edit somewhere else
    let old = state(
        r#"<w:p><w:bookmarkStart w:id="1" w:name="a"/><w:r><w:t>a</w:t></w:r></w:p><w:p><w:r><w:t>b</w:t></w:r></w:p>"#,
    );
    let elsewhere = said(&old, |t| retext(t, 1, "edited"));
    assert!(accepted(&elsewhere).carried_total >= 1);
    // a move needs both of its names, and both of its ends
    let moves = format!(
        r#"<w:p><w:moveFromRangeStart w:id="1" w:name="m" {REV}/><w:moveFrom w:id="3" {REV}><w:r><w:t>a</w:t></w:r></w:moveFrom><w:moveFromRangeEnd w:id="1"/></w:p><w:p><w:moveToRangeStart w:id="2" w:name="m" {REV}/><w:moveTo w:id="4" {REV}><w:r><w:t>a</w:t></w:r></w:moveTo><w:moveToRangeEnd w:id="2"/></w:p>"#
    );
    let s = state(&moves);
    accepted(&said(&s, |t| retext(t, 0, "edited")));
    let renamed = said(&s, |t| {
        let start = nth(t, "moveToRangeStart", 0);
        set_attr(t, start, "name", "n");
    });
    refused(&renamed);
    assert_eq!(renamed.new, [C::MoveNameUnpaired]);
    let open = said(&s, |t| {
        let end = nth(t, "moveToRangeEnd", 0);
        detach(t, end);
    });
    refused(&open);
    assert_eq!(open.new, [C::MoveRangeUnpaired]);
    // a revision id that another part of the document uses is the document's problem
    let header = format!(
        r#"<w:hdr xmlns:w="{W}"><w:p><w:ins w:id="7" {REV}><w:r><w:t>h</w:t></w:r></w:ins></w:p></w:hdr>"#
    );
    let s = state_with(
        r#"<w:p><w:ins w:id="8" w:author="a" w:date="2024-01-01T00:00:00Z"><w:r><w:t>b</w:t></w:r></w:ins></w:p>"#,
        &[("word/header1.xml", "header", "header", &header)],
    );
    let collision = said(&s, |t| {
        let ins = nth(t, "ins", 0);
        set_attr(t, ins, "id", "7");
    });
    refused(&collision);
    assert_eq!(collision.new, [C::RevisionIdDuplicate]);
}
