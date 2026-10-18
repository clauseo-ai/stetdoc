//! The edit laws over any document that opens: batches that need no knowledge of the text (whole paragraphs, refs from the
//! start of the document), so that real documents exercise the pipeline before the view exists to generate batches from.
//!
//! An operation on a real document may be refused, because a paragraph is read-only, a field, a control or another author's
//! pending change, and a refusal is not a failure. What fails is an `internal` error (a bug), a package that a successful edit
//! saves and that no longer opens, a dry run that is not the real edit's receipt, a failed batch that changes the document, a
//! tracked edit that rejecting its author's revisions does not take back, and **output validity**: an edit refused with `validation_failed`
//! (a violation that the document's input did not have is a bug of the compiler, whatever problems the input had), and an output
//! with a finding of a code that the input lacked.

use super::receipts::same_but_for_a_dry_run;
pub use super::sweep::Gap;
use stetdoc_core::{
    Batch, CheckReport, Class, Document, Edit, ErrorCode, FindingCode, Limits, OpenOptions,
    Receipt, Stamp, StetError,
};

/// The refusals that the output validity check calls bugs and that are known, each with whose it is. `laws::sweep::Swept::minus` takes them out of a
/// sweep, and fails when one stops occurring, so an entry goes when its fix does. The two found on the curated corpus when
/// `check` joined the pipeline (a copied mark rPr keyed as a new violation in tdf91095, and w14:textId stamped in the 2007 beta
/// namespace in complex1_NOR) are fixed, and so are the two that step 7's `paragraph` batch found (tdf119136 and tdf97090, a
/// property placed into a `w:pPr` already out of order) no longer occur; `check` now keys an out-of-order violation in a property
/// container by the container, so a placement that shifts which child is named is carried. The list is empty.
pub const KNOWN_GAPS: &[Gap] = &[];

/// The refusals of the same law on the synthetic fixtures that are known, by fixture name. The fixture test takes them out and
/// fails when one stops occurring. `word-misordered` was one, for the same reason as tdf119136, and is fixed.
pub const KNOWN_FIXTURE_GAPS: &[Gap] = &[];

/// The fixed batches, the smoke set beside the random ones of `laws::random`, as JSON operations. Each runs on a fresh document.
const BATCHES: &[&str] = &[
    r#"{"op":"replace","ref":"p1","with":"stetdoc was here"}"#,
    r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["One new paragraph.","Another one."]}"#,
    r#"{"op":"delete","ref":"p1"}"#,
    r#"{"op":"split","ref":"p1","at":1}"#,
    r#"{"op":"merge","ref":"p1"}"#,
    r#"{"op":"replace","ref":"p2","with":"second"},{"op":"accept","all":true},{"op":"replace","ref":"p2","with":"third"}"#,
    r#"{"op":"replace","ref":"p2","with":"second"},{"op":"reject","all":true}"#,
    r#"{"op":"comment","ref":"p1","text":"A comment from stetdoc."}"#,
    r#"{"op":"comment","ref":"p1","text":"A comment."},{"op":"delete","ref":"p1"}"#,
    r#"{"op":"format","ref":"p1","bold":true,"size":"12pt"}"#,
    r#"{"op":"paragraph","ref":"p1","align":"center","space_after":"6pt"}"#,
    r#"{"op":"set","ref":"p1","props":[{"name":"keepNext"}]}"#,
];

fn stamp() -> Stamp {
    Stamp::new("Clauseo", "2026-10-06T10:00:00")
}

fn batch(doc: &Document, ops: &str) -> Result<Batch, String> {
    Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    )
    .map_err(|e| format!("the batch does not parse: {e}"))
}

/// The codes of the violations in a report, each once. A note is legal: an edit may leave a comment with an empty range, as
/// accepting the deletion of what it was anchored to does, and `reconcile` says so with a warning.
fn codes(report: &CheckReport) -> Vec<FindingCode> {
    report
        .by_code
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| c.class() == Class::Violation)
        .collect()
}

/// A batch that went through: the document after it, its receipt, what `save` wrote and the document resumed from its export.
pub struct Applied {
    /// The document after the edit.
    pub doc: Document,
    /// The receipt of the real edit.
    pub receipt: Receipt,
    /// The bytes that `save` wrote after the edit.
    pub saved: Vec<u8>,
    /// The document resumed from the export after the edit, a second copy to accept on.
    pub resumed: Document,
}

/// Runs one batch (the JSON `ops`, without the brackets) on a fresh document, with the laws every batch keeps: a dry run changes
/// nothing and is the real edit's receipt (H14), a refusal is never `internal` or `validation_failed` and changes nothing, the
/// output has no violation code the input lacked (output validity), what was written saves and opens again, and export, resume, export is
/// stable (L4). `input` is the violation codes of the document as it opens. `Ok(Err(e))` is a refusal with its error.
pub fn run_batch(
    bytes: &[u8],
    input: &[FindingCode],
    ops: &str,
    tracked: bool,
) -> Result<Result<Applied, StetError>, String> {
    let options = OpenOptions::default();
    let mut doc = Document::open(bytes.to_vec(), &options).map_err(|e| format!("open: {e}"))?;
    let before = (doc.base(), doc.export().map_err(|e| e.to_string())?);
    let b = match Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    ) {
        Ok(b) => b,
        Err(e) if matches!(e.code(), ErrorCode::Internal | ErrorCode::ValidationFailed) => {
            return Err(format!("the batch does not parse: {e}"));
        }
        Err(e) => return Ok(Err(e)),
    };
    let edit = |b: Batch| {
        let e = Edit::ops(b, stamp());
        if tracked { e } else { e.untracked() }
    };
    // A dry run changes nothing, and fails exactly when the real edit does.
    let dry = doc.edit(edit(b.clone()).dry_run());
    if (doc.base(), doc.export().map_err(|e| e.to_string())?) != before {
        return Err("a dry run changed the document".to_string());
    }
    let real = doc.edit(edit(b));
    match (&dry, &real) {
        (Ok(d), Ok(r)) => same_but_for_a_dry_run("H14", d, r)?,
        (Err(d), Err(r)) => {
            if (d.code(), d.op_index(), d.message()) != (r.code(), r.op_index(), r.message()) {
                return Err(format!(
                    "the dry run failed as {d}, and the real edit as {r}"
                ));
            }
        }
        (d, r) => {
            return Err(format!(
                "the dry run gave {:?} and the real edit {:?}",
                d.as_ref().err(),
                r.as_ref().err()
            ));
        }
    }
    let receipt = match real {
        Err(e) => {
            if matches!(e.code(), ErrorCode::Internal | ErrorCode::ValidationFailed) {
                return Err(e.to_string());
            }
            if (doc.base(), doc.export().map_err(|e| e.to_string())?) != before {
                return Err("a failed batch changed the document".to_string());
            }
            return Ok(Err(e));
        }
        Ok(receipt) => receipt,
    };
    // H14: the anchors of the blocks the dry run made, against the document.
    if let Ok(d) = &dry {
        super::receipts::dry_anchors(&doc, d, &receipt)?;
    }
    // Output validity: nothing in the output that the input did not have.
    if let Some(code) = codes(&doc.check()).into_iter().find(|c| !input.contains(c)) {
        return Err(format!(
            "the output has a `{}` finding that the input lacked",
            code.as_str()
        ));
    }
    // What was written saves, opens again, and exports and resumes to the same document.
    let saved = doc.save().map_err(|e| format!("save: {e}"))?;
    Document::open(saved.bytes.clone(), &options)
        .map_err(|e| format!("the saved package does not open: {e}"))?;
    let wc = doc.export().map_err(|e| format!("export: {e}"))?;
    let resumed = Document::resume(&wc, &options).map_err(|e| format!("resume: {e}"))?;
    if resumed.base() != doc.base() || resumed.export().map_err(|e| e.to_string())? != wc {
        return Err("export, resume, export is not stable after an edit".to_string());
    }
    Ok(Ok(Applied {
        doc,
        receipt,
        saved: saved.bytes,
        resumed,
    }))
}

/// The violation codes of a document as it opens.
pub fn input_codes(bytes: &[u8]) -> Result<Vec<FindingCode>, String> {
    Ok(codes(
        &Document::open(bytes.to_vec(), &OpenOptions::default())
            .map_err(|e| format!("open: {e}"))?
            .check(),
    ))
}

/// Runs the fixed batches (the smoke set) on a document that opens, and returns what broke.
pub fn smoke(bytes: &[u8]) -> Result<(), String> {
    let input = input_codes(bytes)?;
    for ops in BATCHES {
        let applied = run_batch(bytes, &input, ops, true).map_err(|e| format!("{ops}: {e}"))?;
        let Ok(Applied {
            mut doc, receipt, ..
        }) = applied
        else {
            continue;
        };
        // Rejecting what the author wrote takes it back.
        if receipt.tracked && !receipt.revisions.is_empty() {
            let b = batch(&doc, r#"{"op":"reject","author":"Clauseo"}"#)?;
            doc.edit(Edit::ops(b, stamp()))
                .map_err(|e| format!("{ops}: reject: {e}"))?;
            let left = doc
                .revisions(&Default::default())
                .map_err(|e| e.to_string())?;
            if left.iter().any(|r| r.author.as_deref() == Some("Clauseo")) {
                return Err(format!("{ops}: rejecting the author's revisions left some"));
            }
        }
    }
    Ok(())
}
