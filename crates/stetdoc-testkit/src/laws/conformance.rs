//! The operation conformance suite (design `document-and-ops.md` 9.2).
//!
//! Every operation in `capabilities().ops` runs it, and it **fails with the operation's name** when an operation has no case
//! and is not on the list of those that this release does not build, so an operation cannot be added without one. A case is a
//! document, the operation, and a scope that holds its target and one that does not. The suite checks, for each: that the batch
//! `[op, a failing op]` leaves the document as it was (atomic failure); that a dry run leaves it as it was and gives the real
//! edit's receipt apart from the three things a dry run changes; that a scope naming the target passes and one naming another
//! block is `scope_violation` with the operation's index; that the revisions a batch wrote are in its receipt (H3); that
//! rejecting what an operation wrote leaves no revision of its author; and that two runs are the same bytes.

use super::receipts::same_but_for_a_dry_run;
use crate::fixtures::contract;
use stetdoc_core::{
    Batch, Document, Edit, ErrorCode, Limits, OpenOptions, RevisionFilter, Stamp, capabilities,
};

/// One operation, on the contract fixture.
pub struct Case {
    /// The name of the operation, as `capabilities().ops` has it.
    pub op: &'static str,
    /// Operations applied first, as JSON, by the same author. They give `accept` and `reject` something to resolve.
    pub prelude: &'static str,
    /// The operation, as JSON.
    pub ops: &'static str,
    /// A scope that holds what the operation changes.
    pub inside: &'static [&'static str],
    /// A scope that does not.
    pub outside: &'static [&'static str],
    /// True for a case that runs on the contract with a footnote and styles (`contract::package_rich`).
    pub rich: bool,
}

/// The cases of the operations that this release builds. `accept` and `reject` name revision ids, because under a scope `all`
/// and `author` are narrowed to the scope and are never refused, and an id outside it is. The prelude's first change has the
/// ids 0 and 1 in the fixture, which has no revision of its own.
pub const CASES: &[Case] = &[
    Case {
        op: "replace",
        prelude: "",
        ops: r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "insert",
        prelude: "",
        ops: r#"{"op":"insert","ref":"p4","place":"after","paragraphs":["First new.","Second new."]}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "delete",
        prelude: "",
        ops: r#"{"op":"delete","ref":"p4","find":"within thirty days"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "split",
        prelude: "",
        ops: r#"{"op":"split","ref":"p4","before":"due"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "merge",
        prelude: "",
        ops: r#"{"op":"merge","ref":"p4","second":"p5"}"#,
        inside: &["p4..p5"],
        outside: &["p4"],
        rich: false,
    },
    Case {
        op: "row",
        prelude: "",
        ops: r#"{"op":"row","ref":"r1","action":"insert","cells":["x","y"]}"#,
        inside: &["t1"],
        outside: &["p4"],
        rich: false,
    },
    Case {
        op: "accept",
        prelude: r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
        ops: r#"{"op":"accept","ids":[0]}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "reject",
        prelude: r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
        ops: r#"{"op":"reject","ids":[1]}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "comment",
        prelude: "",
        ops: r#"{"op":"comment","ref":"p4","find":"thirty","text":"Too short."}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "reply",
        prelude: r#"{"op":"comment","ref":"p4","text":"Prelude."}"#,
        ops: r#"{"op":"reply","comment":"c0","text":"Agreed."}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "resolve",
        prelude: r#"{"op":"comment","ref":"p4","text":"Prelude."}"#,
        ops: r#"{"op":"resolve","comment":"c0"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "edit-comment",
        prelude: r#"{"op":"comment","ref":"p4","text":"Prelude."}"#,
        ops: r#"{"op":"edit-comment","comment":"c0","text":"Rewritten."}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "delete-comment",
        prelude: r#"{"op":"comment","ref":"p4","text":"Prelude."}"#,
        ops: r#"{"op":"delete-comment","comment":"c0"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "delete-note",
        prelude: "",
        ops: r#"{"op":"delete-note","note":"fn1"}"#,
        inside: &["p5"],
        outside: &["p4"],
        rich: true,
    },
    Case {
        op: "format",
        prelude: "",
        ops: r#"{"op":"format","ref":"p4","find":"thirty days","bold":true,"size":"12pt"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "paragraph",
        prelude: "",
        ops: r#"{"op":"paragraph","ref":"p4","align":"center","indent_left":"0.5in","space_after":"6pt"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "style",
        prelude: "",
        ops: r#"{"op":"style","ref":"p4","style":"Heading 1"}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: true,
    },
    Case {
        op: "list",
        prelude: "",
        ops: r#"{"op":"list","ref":"p4","remove":true}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
    Case {
        op: "set",
        prelude: "",
        ops: r#"{"op":"set","ref":"p4","props":[{"name":"jc","val":"center"},{"name":"spacing","attrs":{"before":"12pt"}}]}"#,
        inside: &["p4"],
        outside: &["p5"],
        rich: false,
    },
];

/// The operations that this release does not build yet, with an instance of each: the suite checks that each is refused as
/// `unsupported`, so the list must shrink as the operations land, and a case replaces the entry. Every operation is built.
pub const UNBUILT: &[(&str, &str)] = &[];

fn stamp() -> Stamp {
    Stamp::new("Clauseo", "2026-10-06T10:00:00")
}

fn open() -> Result<Document, String> {
    Document::open(contract::package(), &OpenOptions::default()).map_err(|e| format!("open: {e}"))
}

fn batch(doc: &Document, ops: &str) -> Result<Batch, String> {
    Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    )
    .map_err(|e| format!("the case does not parse: {e}"))
}

/// A document with the case's prelude applied.
fn prepared(case: &Case) -> Result<Document, String> {
    let mut doc = if case.rich {
        Document::open(contract::package_rich(), &OpenOptions::default())
            .map_err(|e| format!("open: {e}"))?
    } else {
        open()?
    };
    if !case.prelude.is_empty() {
        let b = batch(&doc, case.prelude)?;
        doc.edit(Edit::ops(b, stamp()))
            .map_err(|e| format!("{}: the prelude: {e}", case.op))?;
    }
    Ok(doc)
}

fn ids(doc: &Document) -> Result<Vec<i64>, String> {
    let revisions = doc
        .revisions(&RevisionFilter::default())
        .map_err(|e| format!("revisions: {e}"))?;
    Ok(revisions.into_iter().flat_map(|r| r.ids).collect())
}

/// Checks one case.
pub fn check_case(case: &Case) -> Result<(), String> {
    let op = case.op;
    // Atomic failure: the operation, then one that cannot be applied.
    let mut doc = prepared(case)?;
    let (base, export) = (doc.base(), doc.export().map_err(|e| e.to_string())?);
    let b = batch(
        &doc,
        &format!(r#"{},{{"op":"replace","ref":"p99","with":"x"}}"#, case.ops),
    )?;
    let refused = doc.edit(Edit::ops(b, stamp()));
    match refused {
        Err(e) if e.code() == ErrorCode::NotFound => {}
        other => {
            return Err(format!(
                "{op}: a batch with a failing operation gave {:?}",
                other.err().map(|e| e.to_string())
            ));
        }
    }
    if doc.base() != base || doc.export().map_err(|e| e.to_string())? != export {
        return Err(format!("{op}: a failed batch changed the document"));
    }
    // A dry run changes nothing and is the real edit's receipt.
    let mut dry = prepared(case)?;
    let mut real = prepared(case)?;
    let (base, export) = (dry.base(), dry.export().map_err(|e| e.to_string())?);
    let b = batch(&dry, case.ops)?;
    let dry_receipt = dry
        .edit(Edit::ops(b, stamp()).dry_run())
        .map_err(|e| format!("{op}: dry run: {e}"))?;
    if dry.base() != base || dry.export().map_err(|e| e.to_string())? != export {
        return Err(format!("{op}: a dry run changed the document"));
    }
    if dry_receipt.base != base {
        return Err(format!("{op}: a dry run's base is not the current one"));
    }
    let before = ids(&real)?;
    let b = batch(&real, case.ops)?;
    let receipt = real
        .edit(Edit::ops(b, stamp()))
        .map_err(|e| format!("{op}: {e}"))?;
    same_but_for_a_dry_run(op, &dry_receipt, &receipt)?;
    // The edit was checked, and a clean document is clean after it (output validity).
    if receipt.operations.iter().any(|o| o.changed) && receipt.validation.units == 0 {
        return Err(format!(
            "{op}: an edit that changed the document validated no unit"
        ));
    }
    if !real.check().ok {
        return Err(format!(
            "{op}: an edit of a clean document left it with a violation"
        ));
    }
    // H3: every revision that the batch wrote is in the receipt.
    let receipt_ids: Vec<i64> = receipt
        .revisions
        .iter()
        .flat_map(|a| a.changes.iter().flat_map(|c| c.ids.iter().copied()))
        .collect();
    for id in ids(&real)? {
        if !before.contains(&id) && !receipt_ids.contains(&id) {
            return Err(format!(
                "{op}: revision #{id} was written and is not in the receipt"
            ));
        }
    }
    // The scope: one that holds the target passes, and one that does not is refused with the index of the operation.
    let mut inside = prepared(case)?;
    let b = batch(&inside, case.ops)?;
    let scope: Vec<String> = case.inside.iter().map(|s| (*s).to_string()).collect();
    inside
        .edit(Edit::ops(b, stamp()).scope(scope))
        .map_err(|e| format!("{op}: inside the scope: {e}"))?;
    let mut outside = prepared(case)?;
    let b = batch(&outside, case.ops)?;
    let scope: Vec<String> = case.outside.iter().map(|s| (*s).to_string()).collect();
    match outside.edit(Edit::ops(b, stamp()).scope(scope)) {
        Err(e) if e.code() == ErrorCode::ScopeViolation && e.op_index().is_some() => {}
        other => {
            return Err(format!(
                "{op}: outside the scope gave {:?}",
                other.map(|r| r.text).map_err(|e| e.to_string())
            ));
        }
    }
    // Reversible operations: rejecting everything the author wrote leaves none of it.
    let reversible = capabilities()
        .ops
        .iter()
        .find(|o| o.name == op)
        .is_some_and(|o| o.reversible);
    if reversible {
        let mut doc = prepared(case)?;
        let b = batch(&doc, case.ops)?;
        doc.edit(Edit::ops(b, stamp()))
            .map_err(|e| format!("{op}: {e}"))?;
        let b = batch(&doc, r#"{"op":"reject","author":"Clauseo"}"#)?;
        doc.edit(Edit::ops(b, stamp()))
            .map_err(|e| format!("{op}: reject: {e}"))?;
        if !ids(&doc)?.is_empty() {
            return Err(format!(
                "{op}: rejecting what the operation wrote left revisions"
            ));
        }
    }
    // Determinism: the same bytes, batch and stamp give the same receipt and the same package.
    let (mut a, mut b) = (prepared(case)?, prepared(case)?);
    let ra = a
        .edit(Edit::ops(batch(&a, case.ops)?, stamp()))
        .map_err(|e| e.to_string())?;
    let rb = b
        .edit(Edit::ops(batch(&b, case.ops)?, stamp()))
        .map_err(|e| e.to_string())?;
    if ra != rb
        || a.export().map_err(|e| e.to_string())? != b.export().map_err(|e| e.to_string())?
    {
        return Err(format!("{op}: two runs of one batch differ"));
    }
    Ok(())
}

/// Property operations that would write or remove revision markup, or a property that a tracked change cannot record: each must
/// be refused with `invalid_payload` and change nothing. The second field says whether an untracked edit is refused too (a section
/// break is untracked-only, and so allowed there).
pub const FORGERIES: &[(&str, bool)] = &[
    (
        r#"{"op":"set","ref":"p4","props":[{"name":"pPrChange","attrs":{"author":"Opposing","id":"90","date":"2026-01-01T00:00:00Z"},"children":[{"name":"pPr"}]}]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"p4","find":"thirty","props":[{"name":"rPrChange","attrs":{"author":"Opposing","id":"91"}}]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"p4","target":"mark","props":[{"name":"del","attrs":{"author":"Opposing","id":"92"}}]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"p4","props":[{"name":"pBdr","children":[{"name":"ins","attrs":{"author":"Opposing"}}]}]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"p4","target":"mark","props":[{"name":"b"}],"clear":["ins"]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"p4","props":[{"name":"keepNext"}],"clear":["pPrChange"]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"r1","cell":1,"props":[{"name":"cellDel"}]}"#,
        true,
    ),
    (
        r#"{"op":"set","ref":"t1","props":[{"name":"tblGridChange"}]}"#,
        true,
    ),
    (r#"{"op":"format","ref":"p4","clear":["rPrChange"]}"#, true),
    (
        r#"{"op":"set","ref":"p4","props":[{"name":"sectPr"}]}"#,
        false,
    ),
];

/// Checks that every forgery is refused, tracked and, where it says so, untracked, and that a refused one changes nothing.
pub fn check_forgeries() -> Result<(), String> {
    for (ops, untracked_too) in FORGERIES {
        for tracked in [true, false] {
            if !tracked && !untracked_too {
                continue;
            }
            let mut doc = open()?;
            let before = doc.export().map_err(|e| e.to_string())?;
            let b = batch(&doc, ops)?;
            let edit = if tracked {
                Edit::ops(b, stamp())
            } else {
                Edit::ops(b, stamp()).untracked()
            };
            match doc.edit(edit) {
                Err(e) if e.code() == ErrorCode::InvalidPayload => {}
                other => {
                    return Err(format!(
                        "{ops} (tracked: {tracked}) gave {:?}",
                        other.map(|r| r.text).map_err(|e| e.to_string())
                    ));
                }
            }
            if doc.export().map_err(|e| e.to_string())? != before {
                return Err(format!("{ops}: a refused forgery changed the document"));
            }
        }
    }
    Ok(())
}

/// Checks that every operation of `capabilities()` has a case or is listed as unbuilt, and that the unbuilt ones say so.
pub fn check_coverage() -> Result<(), String> {
    for spec in capabilities().ops {
        let built = CASES.iter().any(|c| c.op == spec.name);
        let unbuilt = UNBUILT.iter().any(|(name, _)| *name == spec.name);
        if built == unbuilt {
            return Err(format!(
                "the operation `{}` {}",
                spec.name,
                if built {
                    "has a case and is also listed as unbuilt"
                } else {
                    "has no conformance case and is not listed as unbuilt"
                }
            ));
        }
    }
    for name in CASES
        .iter()
        .map(|c| c.op)
        .chain(UNBUILT.iter().map(|(n, _)| *n))
    {
        if !capabilities().ops.iter().any(|o| o.name == name) {
            return Err(format!("`{name}` is not an operation of capabilities()"));
        }
    }
    for (name, ops) in UNBUILT {
        let mut doc = open()?;
        let before = doc.export().map_err(|e| e.to_string())?;
        let b = batch(&doc, ops)?;
        match doc.edit(Edit::ops(b, stamp())) {
            Err(e) if e.code() == ErrorCode::Unsupported => {}
            other => {
                return Err(format!(
                    "`{name}` is listed as unbuilt, and gave {:?}",
                    other.map(|r| r.text).map_err(|e| e.to_string())
                ));
            }
        }
        if doc.export().map_err(|e| e.to_string())? != before {
            return Err(format!(
                "`{name}`: an unsupported operation changed the document"
            ));
        }
    }
    Ok(())
}
