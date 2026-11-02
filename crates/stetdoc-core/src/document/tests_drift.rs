//! The cross-references an edit moves (`ref_retargeted`, `ref_unresolved`) and the definitions it removes
//! (`definition_removed`), through `Document`: a synthetic contract shaped like the Harvey LAB supply agreement, whose
//! definitions cite the limbs of clause 16.2 by typed number while Word numbers the limbs.

use super::tests_edit::{batch, run, stamp};
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use crate::api::view::{ViewSpec, Warning, WarningCode};
use crate::index::testing::{Fixture, W, p};

/// A paragraph of the list `num`.
fn limb(num: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{num}\"/></w:numPr></w:pPr><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>"
    )
}

/// Two lists of lower-case letters in brackets, `numId` 1 and 2, each its own abstract list.
fn numbering() -> String {
    let abs = |id: u32| {
        format!(
            "<w:abstractNum w:abstractNumId=\"{id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>\
             <w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"lowerLetter\"/><w:lvlText w:val=\"(%1)\"/></w:lvl></w:abstractNum>"
        )
    };
    format!(
        "<w:numbering xmlns:w=\"{W}\">{}{}<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>\
         <w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>",
        abs(0),
        abs(1)
    )
}

/// The contract, with `extra` paragraphs after the definitions. p1 to p4 are definitions that cite 16.2(a) to 16.2(d), p5 to
/// p9 clause 15 with a list nothing cites, p10 to p12 the headings of clause 16, p13 to p16 the limbs (a) to (d) of 16.2, and
/// p17 clause 17.
fn contract_with(extra: &[String]) -> Document {
    let mut body = [
        p("“Breach Notice” has the meaning given in Clause 16.2(a)."),
        p("“Cure Period” has the meaning given in Clause 16.2(b)."),
        p("“Termination Notice” has the meaning given in Clause 16.2(c)."),
        p("“Total Period” means the period described in Clause 16.2(d)."),
    ]
    .concat();
    body.push_str(&extra.concat());
    body.push_str(
        &[
            p("15. PAYMENT"),
            p("15.1 Invoices. The Seller invoices the Buyer as follows:"),
            limb(2, "monthly, for the Products delivered;"),
            limb(2, "on completion, for the services;"),
            limb(2, "on demand, for the costs agreed in writing."),
            p("16. TERM AND TERMINATION"),
            p("16.1 Term. This Agreement continues for five years."),
            p("16.2 Termination for Cause. Either Party may terminate this Agreement as follows:"),
            limb(1, "Step 1 — Breach Notice. The Non-Breaching Party shall give the Breaching Party a Breach Notice."),
            limb(1, "Step 2 — Cure Period. Upon receipt of a Breach Notice, the Breaching Party shall have thirty (30) days to cure the breach (the “Cure Period”). During the Cure Period, the Breaching Party shall report weekly."),
            limb(1, "Step 3 — Termination Notice. If the breach is not cured within the Cure Period, the Non-Breaching Party may give a Termination Notice."),
            limb(1, "Minimum Total Period. The Cure Period and the notice period together last at least ninety (90) days."),
            p("17. GENERAL"),
        ]
        .concat(),
    );
    let bytes = Fixture::new(&body)
        .part("numbering", "word/numbering.xml", numbering())
        .bytes();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

fn contract() -> Document {
    contract_with(&[])
}

/// The receipts of a dry run and of the real edit of the same operations on two copies of the contract, after they agree
/// on everything this check adds.
fn both(make: impl Fn() -> Document, ops: &str) -> (Receipt, Document) {
    let mut dry_doc = make();
    let b = batch(&dry_doc, ops);
    let dry = dry_doc.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    let mut doc = make();
    let real = run(&mut doc, ops);
    assert_eq!(dry.warnings, real.warnings, "the dry run's warnings");
    assert_eq!(dry.ref_drift, real.ref_drift, "the dry run's ref_drift");
    (real, doc)
}

/// The warnings of the three codes, as `code block: message`.
fn xrefs(r: &Receipt) -> Vec<String> {
    r.warnings
        .iter()
        .filter(|w| is_xref(w))
        .map(|w| {
            format!(
                "{} {}: {}",
                w.code.as_str(),
                w.block.as_deref().unwrap_or("-"),
                w.message
            )
        })
        .collect()
}

fn is_xref(w: &Warning) -> bool {
    matches!(
        w.code,
        WarningCode::RefRetargeted | WarningCode::RefUnresolved | WarningCode::DefinitionRemoved
    )
}

/// The view of the whole body, with the mention lines.
fn view(doc: &Document) -> String {
    doc.view(&ViewSpec::default()).unwrap().text
}

const B: &str = "16.2(b) Step 2 — Cure Period. Upon receipt of a B…";
const C: &str = "16.2(c) Step 3 — Termination Notice. If the breac…";
const D: &str = "16.2(d) Minimum Total Period. The Cure Period and…";

#[test]
fn the_definitions_cite_the_limbs_that_word_numbers() {
    let text = view(&contract());
    for line in [
        "{~ \"Clause 16.2(a)\" -> p13 (16.2(a) Step 1 — Breach Notice. The Non-Breaching…)}",
        &format!("{{~ \"Clause 16.2(b)\" -> p14 ({B})}}"),
        &format!("{{~ \"Clause 16.2(c)\" -> p15 ({C})}}"),
        &format!("{{~ \"Clause 16.2(d)\" -> p16 ({D})}}"),
    ] {
        assert!(text.contains(line), "{line}\n{text}");
    }
}

/// The labels of the limbs after one is inserted after (a) or removed from (b).
const B_AS_C: &str = "16.2(c) Step 2 — Cure Period. Upon receipt of a B…";
const C_AS_D: &str = "16.2(d) Step 3 — Termination Notice. If the breac…";
const C_AS_B: &str = "16.2(b) Step 3 — Termination Notice. If the breac…";
const D_AS_C: &str = "16.2(c) Minimum Total Period. The Cure Period and…";
const INSERT_1A: &str = r#"{"op":"insert","ref":"p13","place":"after","paragraphs":["Step 1A — Acknowledgement. The Breaching Party shall acknowledge the Breach Notice within five (5) Business Days."]}"#;
const NEXT_MOVED: &str = "update the reference in this batch, or confirm it is meant to move.";

#[test]
fn a_limb_inserted_after_a_moves_every_reference_to_the_limbs_after_it_and_the_dry_run_agrees() {
    let (r, _) = both(contract, INSERT_1A);
    let new = "16.2(b) Step 1A — Acknowledgement. The Breaching…";
    assert_eq!(
        xrefs(&r),
        [
            format!(
                "ref_retargeted p2: \"Clause 16.2(b)\" pointed at p14 ({B}), now a block this batch adds ({new})."
            ),
            format!(
                "ref_retargeted p3: \"Clause 16.2(c)\" pointed at p15 ({C}), now p14 ({B_AS_C})."
            ),
            format!(
                "ref_retargeted p4: \"Clause 16.2(d)\" pointed at p16 ({D}), now p15 ({C_AS_D})."
            ),
        ],
        "nothing for p1, whose 16.2(a) did not move"
    );
    assert!(
        r.warnings
            .iter()
            .filter(|w| is_xref(w))
            .all(|w| w.next.as_deref() == Some(NEXT_MOVED))
    );
    assert_eq!(r.ref_drift.len(), 3);
    let first = &r.ref_drift[0];
    assert_eq!(
        (first.source.as_str(), first.text.as_str()),
        ("p2", "Clause 16.2(b)")
    );
    assert_eq!(
        (first.before.r#ref.as_deref(), first.before.label.as_str()),
        (Some("p14"), B)
    );
    let after = first.after.as_ref().expect("one block after");
    assert_eq!((after.r#ref.as_deref(), after.label.as_str()), (None, new));
    assert_eq!(
        r.ref_drift[1]
            .after
            .as_ref()
            .and_then(|a| a.r#ref.as_deref()),
        Some("p14")
    );
    let head = r.text.lines().next().unwrap();
    assert!(
        head.contains(". 3 cross-reference warnings below (3 ref_retargeted). base "),
        "{head}"
    );
    assert!(
        r.text.contains(&format!(
            "warning ref_retargeted p2: \"Clause 16.2(b)\" pointed at p14 ({B})"
        )),
        "{}",
        r.text
    );
}

#[test]
fn a_tracked_deletion_of_b_moves_the_references_and_orphans_the_term_it_defined() {
    let (r, doc) = both(contract, r#"{"op":"delete","ref":"p14"}"#);
    assert_eq!(
        xrefs(&r),
        [
            format!("ref_retargeted p2: \"Clause 16.2(b)\" pointed at p14 ({B}), now p15 ({C_AS_B})."),
            format!("ref_retargeted p3: \"Clause 16.2(c)\" pointed at p15 ({C}), now p16 ({D_AS_C})."),
            format!("ref_unresolved p4: \"Clause 16.2(d)\" pointed at p16 ({D}) and now resolves to nothing."),
            "definition_removed p14: defined \"Cure Period\", and no block defines it now. It is still used in p15 and p16, and p2 cites this block.".to_string(),
        ]
    );
    let removed = r
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::DefinitionRemoved)
        .unwrap();
    assert_eq!(
        removed.next.as_deref(),
        Some("move the definition to a block that stays, or update the uses.")
    );
    let unresolved = r
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::RefUnresolved)
        .unwrap();
    assert_eq!(
        unresolved.next.as_deref(),
        Some("update the reference in this batch, or remove it.")
    );
    assert_eq!(
        r.ref_drift.len(),
        3,
        "one for each warning about a reference"
    );
    assert_eq!(r.ref_drift[2].after, None);
    assert!(
        r.text.lines().next().unwrap().contains(
            ". 4 cross-reference warnings below (2 ref_retargeted, 1 ref_unresolved, 1 definition_removed). base "
        ),
        "{}",
        r.text
    );
    // The view agrees: the mention now names the next limb.
    assert!(view(&doc).contains(&format!("{{~ \"Clause 16.2(b)\" -> p15 ({C_AS_B})}}")));
}

#[test]
fn a_reference_the_batch_also_rewrites_is_not_drift() {
    let ops = r#"{"op":"delete","ref":"p14"},{"op":"replace","ref":"p2","find":"16.2(b)","with":"16.2(a)"}"#;
    let (r, _) = both(contract, ops);
    assert!(
        r.warnings
            .iter()
            .filter(|w| is_xref(w))
            .all(|w| w.block.as_deref() != Some("p2")),
        "{:?}",
        xrefs(&r)
    );
    assert!(r.ref_drift.iter().all(|d| d.source != "p2"));
    assert!(!xrefs(&r).is_empty(), "the other references still moved");
}

#[test]
fn a_renumbering_that_no_reference_points_into_says_nothing() {
    let ops =
        r#"{"op":"insert","ref":"p7","place":"after","paragraphs":["weekly, for the samples;"]}"#;
    let (r, doc) = both(contract, ops);
    assert_eq!(xrefs(&r), Vec::<String>::new());
    assert!(r.ref_drift.is_empty());
    assert!(!r.text.contains("cross-reference"), "{}", r.text);
    assert!(
        view(&doc).contains("{§\"(d)\"} on demand"),
        "the list was renumbered"
    );
}

#[test]
fn rejecting_the_deletion_puts_every_reference_back_and_says_nothing() {
    let mut doc = contract();
    run(&mut doc, r#"{"op":"delete","ref":"p14"}"#);
    let r = run(&mut doc, r#"{"op":"reject","author":"Clauseo"}"#);
    assert_eq!(xrefs(&r), Vec::<String>::new());
    assert!(r.ref_drift.is_empty());
    let text = view(&doc);
    for line in [
        format!("{{~ \"Clause 16.2(b)\" -> p14 ({B})}}"),
        format!("{{~ \"Clause 16.2(c)\" -> p15 ({C})}}"),
        format!("{{~ \"Clause 16.2(d)\" -> p16 ({D})}}"),
    ] {
        assert!(text.contains(&line), "{line}\n{text}");
    }
}

#[test]
fn a_list_operation_that_takes_a_limb_out_of_the_list_moves_the_references() {
    let (r, _) = both(contract, r#"{"op":"list","ref":"p14","remove":true}"#);
    let x = xrefs(&r);
    assert!(
        x.contains(&format!(
            "ref_retargeted p2: \"Clause 16.2(b)\" pointed at p14 ({B}), now p15 ({C_AS_B})."
        )),
        "{x:?}"
    );
    assert!(
        x.iter().any(|w| w.starts_with("ref_retargeted p3: ")),
        "{x:?}"
    );
    assert!(
        x.iter().any(|w| w.starts_with("ref_unresolved p4: ")),
        "{x:?}"
    );
    assert!(
        !x.iter().any(|w| w.starts_with("definition_removed")),
        "the limb is still there: {x:?}"
    );
}

#[test]
fn deleting_d_leaves_its_reference_resolving_to_nothing() {
    let (r, _) = both(contract, r#"{"op":"delete","ref":"p16"}"#);
    assert_eq!(
        xrefs(&r),
        [format!(
            "ref_unresolved p4: \"Clause 16.2(d)\" pointed at p16 ({D}), which this edit deletes, and now resolves to nothing."
        )]
    );
    assert_eq!(r.ref_drift.len(), 1);
    assert!(
        r.text
            .lines()
            .next()
            .unwrap()
            .contains(". 1 cross-reference warning below (1 ref_unresolved). base "),
        "{}",
        r.text
    );
}

#[test]
fn an_untracked_deletion_warns_too() {
    let mut doc = contract();
    let b = batch(&doc, r#"{"op":"delete","ref":"p14"}"#);
    let r = doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    let x = xrefs(&r);
    assert!(
        x.contains(&format!(
            "ref_retargeted p2: \"Clause 16.2(b)\" pointed at p14 ({B}), now p15 ({C_AS_B})."
        )),
        "{x:?}"
    );
    assert!(
        x.iter().any(|w| w.starts_with("definition_removed p14: ")),
        "{x:?}"
    );
}

/// The working copy of the contract with the line of p14 taken out of \`document.md\` by hand.
fn without_b(doc: &Document) -> crate::api::working_copy::WorkingCopy {
    let mut wc = doc.export().unwrap();
    let file = wc
        .files
        .iter_mut()
        .find(|(name, _)| name.ends_with("document.md"))
        .unwrap();
    let kept: Vec<&str> = file.1.lines().filter(|l| !l.starts_with("p14 ")).collect();
    assert_eq!(kept.len() + 1, file.1.lines().count(), "{}", file.1);
    file.1 = kept.join("\n") + "\n";
    wc
}

#[test]
fn check_on_a_working_copy_and_the_edit_of_its_files_warn_as_the_batch_does() {
    let doc = contract();
    let pending = without_b(&doc)
        .pending_files()
        .unwrap()
        .expect("one file is pending");
    let checked = doc
        .check_pending(Some(pending.clone()), Some(stamp()), false)
        .unwrap();
    let receipt = checked.receipt.expect("a receipt");
    let expected = xrefs(&both(contract, r#"{"op":"delete","ref":"p14"}"#).0);
    assert_eq!(xrefs(&receipt), expected);
    assert_eq!(checked.exit_status, 0, "a warning is no failure");
    let mut real = contract();
    let applied = real.edit(Edit::files(pending, stamp())).unwrap();
    assert_eq!(xrefs(&applied), expected);
    assert_eq!(applied.ref_drift, receipt.ref_drift);
}

#[test]
fn a_receipt_shows_eight_warnings_about_references_and_says_how_many_more_the_json_has() {
    let extra: Vec<String> = (1..=10)
        .map(|i| p(&format!("Item {i} is subject to Clause 16.2(b).")))
        .collect();
    let make = || contract_with(&extra);
    // The ten items come before the headings, so limb (a) is p23.
    let (r, _) = both(make, &INSERT_1A.replace("p13", "p23"));
    // Ten items and the three definitions.
    assert_eq!(r.ref_drift.len(), 13);
    assert_eq!(
        r.text.matches("warning ref_retargeted ").count(),
        8,
        "{}",
        r.text
    );
    let more = "and 5 more cross-reference warnings: the JSON receipt (--json) lists every one, with each reference's old and new target in ref_drift.\n";
    assert!(r.text.contains(more), "{}", r.text);
    let head = r.text.lines().next().unwrap();
    assert!(
        head.contains("13 cross-reference warnings below (13 ref_retargeted)"),
        "{head}"
    );
    let short = r.text_within(2_000);
    assert_eq!(short.lines().next(), Some(head));
    assert_eq!(
        short.matches("warning ref_retargeted ").count(),
        8,
        "{short}"
    );
    assert!(short.contains(more), "{short}");
}
