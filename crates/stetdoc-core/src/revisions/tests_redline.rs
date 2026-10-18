//! The compare primitives (design `compare.md`, 6.1): `redline_paragraph`, `mark_inserted`, `place_deleted` and `set_snapshot`.
//! Each case builds the redline of a revised body, with the original's content given as detached nodes, and checks the two
//! compare laws on it with the resolver of this module: accepting everything gives the revised body and rejecting everything
//! the original, both after `normalize`. `finish` runs the compiler's own post-conditions on everything that was written.

use super::compiler::Compiler;
use super::index::ChangeIndex;
use super::redline_types::{ParaRedline, RedlineEdit, RunSnapshot};
use super::resolve::resolve;
use super::stamp::Track;
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::state::Tx;
use crate::wordml::{BlockKey, NodeId};

const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// A detached run in the tree of the transaction, holding `text`, bold if asked.
pub(super) fn detached_run(tx: &mut Tx<'_>, text: &str, bold: bool) -> NodeId {
    let tree = tx.part_mut(MAIN).unwrap();
    let mut ed = tree.edit();
    let run = ed.create_element(WNS, "r").unwrap();
    if bold {
        let rpr = ed.create_element(WNS, "rPr").unwrap();
        let b = ed.create_element(WNS, "b").unwrap();
        ed.append_child(rpr, b).unwrap();
        ed.append_child(run, rpr).unwrap();
    }
    let t = ed.create_element(WNS, "t").unwrap();
    ed.set_attr(t, XML_NS, "space", "preserve").unwrap();
    let node = ed.create_text(text).unwrap();
    ed.append_child(t, node).unwrap();
    ed.append_child(run, t).unwrap();
    run
}

/// A detached paragraph with one run, and its `pPr` from `ppr` (children as XML-free: only `jc` is used here).
pub(super) fn detached_paragraph(tx: &mut Tx<'_>, text: &str, jc: Option<&str>) -> NodeId {
    let run = detached_run(tx, text, false);
    let tree = tx.part_mut(MAIN).unwrap();
    let mut ed = tree.edit();
    let p = ed.create_element(WNS, "p").unwrap();
    if let Some(v) = jc {
        let ppr = ed.create_element(WNS, "pPr").unwrap();
        let j = ed.create_element(WNS, "jc").unwrap();
        ed.set_attr(j, WNS, "val", v).unwrap();
        ed.append_child(ppr, j).unwrap();
        ed.append_child(p, ppr).unwrap();
    }
    ed.append_child(p, run).unwrap();
    p
}

fn step(action: Action) -> Step {
    Step {
        action,
        selector: Selector::All,
    }
}

pub(super) type Effects = Vec<super::outcome::Effect>;

/// Resolves everything of the committed redline in a new transaction and gives the main part in the form of `normalize`.
fn resolved(state: &crate::state::State, action: Action) -> String {
    let mut tx = state.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &[step(action)]).unwrap();
    normalized_part(&tx.parts())
}

/// Opens the revised body, makes the detached nodes with `make`, writes the redline with `apply`, finishes the compiler, and
/// checks the two laws: accepting everything gives the revised body and rejecting everything the original. Returns what `apply`
/// returned and the redline's body.
pub(super) fn check(
    original_body: &str,
    revised_body: &str,
    make: impl FnOnce(&mut Tx<'_>, &[BlockKey]) -> Vec<NodeId>,
    apply: impl FnOnce(&mut Compiler<'_, '_>, &[BlockKey], &[NodeId]) -> Effects,
) -> (Effects, String) {
    check_open(original_body, revised_body, revised_body, make, apply)
}

/// As `check`, with a body to open that holds a stand-in for the original's block, which `make` takes out of the tree.
pub(super) fn check_open(
    original_body: &str,
    revised_body: &str,
    open_body: &str,
    make: impl FnOnce(&mut Tx<'_>, &[BlockKey]) -> Vec<NodeId>,
    apply: impl FnOnce(&mut Compiler<'_, '_>, &[BlockKey], &[NodeId]) -> Effects,
) -> (Effects, String) {
    let s = state(open_body);
    let mut tx = s.begin();
    let all = keys(&tx);
    let detached = make(&mut tx, &all);
    let effects = {
        let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
        let effects = apply(&mut c, &all, &detached);
        let xml = body_xml(c.tx());
        c.finish()
            .unwrap_or_else(|e| panic!("finish refused the redline: {e}\n{xml}"));
        effects
    };
    let xml = body_xml(&tx);
    let added = schema_added(&s, &tx);
    assert!(
        added.is_empty(),
        "the redline adds violations {added:?}\n{xml}"
    );
    let (red, _) = tx.commit().unwrap();
    assert_eq!(
        resolved(&red, Action::Accept),
        normalized_body(revised_body),
        "accepting everything must give the revised body\n{xml}"
    );
    assert_eq!(
        resolved(&red, Action::Reject),
        normalized_body(original_body),
        "rejecting everything must give the original body\n{xml}"
    );
    (effects, xml)
}

pub(super) fn redline(edits: Vec<RedlineEdit>) -> ParaRedline {
    ParaRedline {
        edits,
        ..ParaRedline::default()
    }
}

#[test]
fn a_replaced_word_is_a_deletion_then_an_insertion_and_both_laws_hold() {
    let original = p(&r("The slow brown dog"));
    let revised = p(&r("The quick brown dog"));
    let (fx, xml) = check(
        &original,
        &revised,
        |tx, _| vec![detached_run(tx, "slow", false)],
        |c, all, nodes| {
            let plan = redline(vec![RedlineEdit {
                insert: 4..9,
                deleted: vec![nodes[0]],
            }]);
            vec![c.redline_paragraph(&all[0], &plan).unwrap()]
        },
    );
    assert!(
        xml.find("<w:del").unwrap() < xml.find("<w:ins").unwrap(),
        "{xml}"
    );
    assert!(xml.contains("<w:delText"), "{xml}");
    assert_eq!(fx[0].touched.len(), 1);
}

#[test]
fn a_pure_insertion_marks_the_revised_runs_and_writes_no_deletion() {
    let original = p(&r("The brown dog"));
    let revised = p(&r("The quick brown dog"));
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            vec![
                c.redline_paragraph(
                    &all[0],
                    &redline(vec![RedlineEdit {
                        insert: 4..10,
                        deleted: vec![],
                    }]),
                )
                .unwrap(),
            ]
        },
    );
    assert!(!xml.contains("<w:del"), "{xml}");
    assert!(xml.contains("<w:ins"), "{xml}");
}

#[test]
fn a_pure_deletion_is_placed_at_the_gap_of_the_edit_and_edits_apply_from_the_last_to_the_first() {
    let original = p(&r("Alpha gone beta delta epsilon two gamma"));
    let revised = p(&r("Alpha beta delta new epsilon gamma"));
    // "gone " is deleted after "Alpha ", "new " inserted before "epsilon", " two" deleted before " gamma"
    let (_, xml) = check(
        &original,
        &revised,
        |tx, _| {
            vec![
                detached_run(tx, "gone ", false),
                detached_run(tx, "two ", false),
            ]
        },
        |c, all, nodes| {
            let plan = redline(vec![
                RedlineEdit {
                    insert: 6..6,
                    deleted: vec![nodes[0]],
                },
                RedlineEdit {
                    insert: 17..21,
                    deleted: vec![],
                },
                RedlineEdit {
                    insert: 29..29,
                    deleted: vec![nodes[1]],
                },
            ]);
            vec![c.redline_paragraph(&all[0], &plan).unwrap()]
        },
    );
    assert_eq!(xml.matches("<w:del ").count(), 2, "{xml}");
    assert_eq!(xml.matches("<w:ins ").count(), 1, "{xml}");
}

#[test]
fn deleted_text_keeps_its_own_formatting() {
    let original = p(&format!(
        "{}{}{}",
        r("Keep "),
        "<w:r><w:rPr><w:b/></w:rPr><w:t>bold gone </w:t></w:r>",
        r("end")
    ));
    let revised = p(&r("Keep end"));
    let (_, xml) = check(
        &original.replace(
            "<w:t>bold gone </w:t>",
            r#"<w:t xml:space="preserve">bold gone </w:t>"#,
        ),
        &revised,
        |tx, _| vec![detached_run(tx, "bold gone ", true)],
        |c, all, nodes| {
            vec![
                c.redline_paragraph(
                    &all[0],
                    &redline(vec![RedlineEdit {
                        insert: 5..5,
                        deleted: vec![nodes[0]],
                    }]),
                )
                .unwrap(),
            ]
        },
    );
    assert!(xml.contains("<w:b/>"), "{xml}");
}

#[test]
fn changed_formatting_over_unchanged_text_is_an_rprchange_with_the_old_properties() {
    let original = p(&format!(
        "{}{}",
        r("Payment is due in "),
        r#"<w:r><w:t xml:space="preserve">thirty days</w:t></w:r>"#
    ));
    let revised = p(&format!(
        "{}{}",
        r("Payment is due in "),
        r#"<w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">thirty days</w:t></w:r>"#
    ));
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            let plan = ParaRedline {
                fmt: vec![RunSnapshot {
                    range: 18..29,
                    snapshot: Vec::new(),
                }],
                ..ParaRedline::default()
            };
            vec![c.redline_paragraph(&all[0], &plan).unwrap()]
        },
    );
    assert!(xml.contains("<w:rPrChange"), "{xml}");
}
