//! The laws of a comparison through `Document` (design `compare.md` 9.1): **N5 Validity**, that `compare` passes `check` with the
//! foreign origins and that the saved redline has no finding that neither input has once its revisions are accepted (output validity), and
//! **N7** read through the receipt, that each move names two blocks of the redline.
//!
//! The pair set is the testkit's: until the corpus pairs manifest (`corpus/pairs.tsv`) lands, `pairs` makes pairs from the curated
//! set, a document and the same document after untracked edits made through `Document::edit`, and two different documents.

use stetdoc_core::{
    Batch, Class, CompareOptions, Document, Edit, ErrorCode, FindingCode, FormatId, Limits,
    OpenOptions, Stamp, WarningCode, compare,
};

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

/// The codes of the violations of a document, each once.
fn violations(doc: &Document) -> Vec<FindingCode> {
    doc.check()
        .by_code
        .iter()
        .map(|(c, _)| *c)
        .filter(|c| c.class() == Class::Violation)
        .collect()
}

/// The violations of an input with its revisions accepted, which is what the redline is built from.
fn accepted_violations(bytes: &[u8]) -> Result<Vec<FindingCode>, String> {
    let mut doc = Document::open(bytes.to_vec(), &OpenOptions::default())
        .map_err(|e| format!("open: {e}"))?;
    if let Ok(b) = batch(&doc, r#"{"op":"accept","all":true}"#) {
        // A document that cannot be accepted is read as it is: its own findings are then the ones to compare with.
        let _ = doc.edit(Edit::ops(b, stamp()));
    }
    Ok(violations(&doc))
}

/// What one pair that held the laws gave.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Verdict {
    /// Whether it was compared: false when `compare` refused it for a reason that is not a defect (an input over a limit, a format
    /// this release does not compare).
    pub compared: bool,
    /// The blocks that the receipt names with `compare_keys_merged`: copies whose paragraphs and tables did not pair one to one
    /// with the original's, so that `check` let each keep any problem of any of them.
    pub merged: usize,
    /// The blocks that the receipt names as not shown as changes, or not given back by rejecting.
    pub excluded: usize,
}

/// Holds one pair to N5 and N7, or says which broke. A refusal that is not a defect is a verdict that was not compared.
pub fn validity(original: &[u8], revised: &[u8]) -> Result<Verdict, String> {
    let options = CompareOptions {
        limits: Limits::default(),
        stamp: stamp(),
        format: FormatId::stet(),
    };
    let compared = match compare(original.to_vec(), revised.to_vec(), &options) {
        Ok(c) => c,
        Err(e) if matches!(e.code(), ErrorCode::ValidationFailed | ErrorCode::Internal) => {
            return Err(format!("compare: {e}"));
        }
        Err(_) => return Ok(Verdict::default()),
    };
    let saved = compared.document.save().map_err(|e| format!("save: {e}"))?;
    let redline =
        Document::open(saved.bytes, &OpenOptions::default()).map_err(|e| format!("reopen: {e}"))?;
    let mut known = accepted_violations(original)?;
    known.extend(accepted_violations(revised)?);
    if let Some(code) = violations(&redline)
        .into_iter()
        .find(|c| !known.contains(c))
    {
        return Err(format!(
            "N5: the redline has a `{}` finding that neither input has",
            code.as_str()
        ));
    }
    for m in &compared.receipt.moves {
        if m.from == m.to || !m.from.starts_with('p') || !m.to.starts_with('p') {
            return Err(format!(
                "N7: a move names {} and {}, not two paragraphs of the redline",
                m.from, m.to
            ));
        }
    }
    let count = |code: WarningCode| {
        compared
            .receipt
            .warnings
            .iter()
            .filter(|w| w.code == code)
            .count()
    };
    Ok(Verdict {
        compared: true,
        merged: count(WarningCode::CompareKeysMerged),
        excluded: count(WarningCode::CompareNotRedlined) + count(WarningCode::CompareNotRestored),
    })
}

/// A document after a few untracked edits through `Document::edit`, each applied when the document lets it be: a paragraph
/// rewritten, one inserted and one deleted. `None` when none applied.
pub fn edited(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut doc = Document::open(bytes.to_vec(), &OpenOptions::default()).ok()?;
    let ops = [
        r#"{"op":"replace","ref":"p2","with":"This paragraph was rewritten for the comparison."}"#,
        r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["A paragraph that the revised document adds."]}"#,
        r#"{"op":"delete","ref":"p3"}"#,
    ];
    let mut any = false;
    for ops in ops {
        let Ok(b) = batch(&doc, ops) else { continue };
        any |= doc.edit(Edit::ops(b, stamp()).untracked()).is_ok();
    }
    if !any {
        return None;
    }
    doc.save().ok().map(|s| s.bytes)
}
