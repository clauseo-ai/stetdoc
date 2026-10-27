//! A violation in a property container keeps its key wherever the container stands in the unit:
//! the compiler copies a paragraph mark's `w:rPr` onto a new run and a `w:pPr` into the snapshot of
//! a `w:pPrChange`, and a copy of a defect the unit had is carried (tdf91095, pandoc
//! `trim_last_inline`, docxplate `issue.48`, officeimo Hamlet). A defect that is new is still new, and a
//! counted one (`schema_non_canonical_order`) is new when the unit has more of it than before.

use super::tests::state;
use super::tests_edit::{accepted, append, el, el_with, insert_first, nth, refused, said};
use crate::api::check::FindingCode as C;
use crate::xml::SpanTree;

/// A paragraph whose mark holds `w:sz` twice, with one plain run.
const MARK_TWICE: &str = "<w:p><w:pPr><w:rPr><w:sz w:val=\"30\"/><w:sz w:val=\"30\"/></w:rPr></w:pPr><w:r><w:t>a</w:t></w:r></w:p>";

/// A paragraph whose properties are out of order (`w:jc` before `w:pStyle`), with one plain run.
const PPR_OUT_OF_ORDER: &str = "<w:p><w:pPr><w:jc w:val=\"left\"/><w:pStyle w:val=\"A\"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p>";

/// Gives the first run a copy of the mark's `w:rPr`, as `insert_block` formats new text.
fn copy_mark_onto_run(t: &mut SpanTree) {
    let mark = nth(t, "rPr", 0);
    let copy = t.edit().clone_subtree(mark).expect("clones");
    let run = nth(t, "r", 0);
    insert_first(t, run, copy);
}

/// Writes a `w:pPrChange` whose snapshot is a copy of the paragraph's properties, as `set_props`
/// does.
fn snapshot_ppr(t: &mut SpanTree) {
    let ppr = nth(t, "pPr", 0);
    let copy = t.edit().clone_subtree(ppr).expect("clones");
    let change = el_with(
        t,
        "pPrChange",
        &[
            ("id", "1"),
            ("author", "Clauseo"),
            ("date", "2026-10-05T14:03:27Z"),
        ],
    );
    append(t, change, copy);
    append(t, ppr, change);
    // A real snapshot changes something: the paragraph is now centred.
    let jc = nth(t, "jc", 0);
    t.edit()
        .set_attr(jc, super::tests::W, "val", "center")
        .expect("sets");
}

#[test]
fn a_verbatim_copy_of_the_marks_repeated_property_onto_a_run_is_new() {
    // A repeat is `schema_non_canonical_order`, which is counted: the compiler drops an exact repeat from every container it
    // creates (`revisions::canonical`), so a copy that keeps it adds one.
    let s = state(MARK_TWICE);
    let said = said(&s, copy_mark_onto_run);
    refused(&said);
    assert_eq!(said.new, [C::SchemaNonCanonicalOrder], "{:?}", said.new);
}

#[test]
fn a_snapshot_of_out_of_order_paragraph_properties_is_carried() {
    let s = state(PPR_OUT_OF_ORDER);
    accepted(&said(&s, snapshot_ppr));
}

#[test]
fn a_new_repeated_property_on_a_run_is_new_though_the_mark_repeats_another() {
    let s = state(MARK_TWICE);
    let said = said(&s, |t| {
        let rpr = el(t, "rPr");
        let b1 = el(t, "b");
        let b2 = el(t, "b");
        append(t, rpr, b1);
        append(t, rpr, b2);
        let run = nth(t, "r", 0);
        insert_first(t, run, rpr);
    });
    refused(&said);
    assert_eq!(said.new, [C::SchemaNonCanonicalOrder], "{:?}", said.new);
}

#[test]
fn a_new_defect_in_a_snapshot_is_new() {
    // The paragraph's properties are in order, and the snapshot is not.
    let s = state(
        "<w:p><w:pPr><w:pStyle w:val=\"A\"/><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p>",
    );
    let said = said(&s, |t| {
        let ppr = nth(t, "pPr", 0);
        let snap = el(t, "pPr");
        let jc = el_with(t, "jc", &[("val", "center")]);
        let style = el_with(t, "pStyle", &[("val", "A")]);
        append(t, snap, jc);
        append(t, snap, style);
        let change = el_with(
            t,
            "pPrChange",
            &[
                ("id", "1"),
                ("author", "Clauseo"),
                ("date", "2026-10-05T14:03:27Z"),
            ],
        );
        append(t, change, snap);
        append(t, ppr, change);
    });
    refused(&said);
}

/// Adds `w:spacing` to the paragraph's properties right after `w:jc`, where it is out of order.
fn spacing_after_jc(t: &mut SpanTree) {
    let ppr = nth(t, "pPr", 0);
    let jc = nth(t, "jc", 0);
    let spacing = el_with(t, "spacing", &[("after", "120")]);
    let next = t.node_ref(jc).next_sibling().map(|n| n.id());
    t.edit().insert_before(ppr, next, spacing).expect("inserts");
}

#[test]
fn a_property_placed_into_properties_already_out_of_order_is_carried_whichever_child_is_named() {
    // `w:jc` before `w:pStyle`: the validator names `w:pStyle`. With `w:spacing` after `w:jc` it names `w:spacing` too, and the
    // container is still the one problem it was.
    let s = state(PPR_OUT_OF_ORDER);
    accepted(&said(&s, spacing_after_jc));
}

#[test]
fn properties_newly_out_of_order_are_new() {
    let s = state(
        "<w:p><w:pPr><w:pStyle w:val=\"A\"/><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>a</w:t></w:r></w:p>",
    );
    let said = said(&s, spacing_after_jc);
    refused(&said);
    assert_eq!(said.new, [C::SchemaOutOfOrder], "{:?}", said.new);
}
