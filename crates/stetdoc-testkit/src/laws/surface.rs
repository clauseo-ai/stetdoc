//! The read surface of `Document` is total: for a document that opens, `files`, `view`, `lookup` and `inspect` answer, with
//! nothing in an answer that disagrees with another (the token of every answer is the document's), and no `internal` error.
//!
//! The targets are taken from the files themselves, so the law needs no knowledge of the document: the first and the last block
//! of each kind that `document.md` names, and the three listings.

use stetdoc_core::{Document, OpenOptions, ViewSpec};

/// The refs a file names at the start of a line, in order: `p12`, `t3`, `r4`.
fn refs(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|l| l.split_once(' ').map(|(r, _)| r))
        .filter(|r| {
            let mut c = r.chars();
            matches!(c.next(), Some('p' | 't' | 'r'))
                && c.clone().count() > 0
                && c.all(|x| x.is_ascii_digit())
        })
        .collect()
}

/// Checks the surface of one document.
pub fn smoke(bytes: &[u8]) -> Result<(), String> {
    let doc = Document::open(bytes.to_vec(), &OpenOptions::default())
        .map_err(|e| format!("open: {e}"))?;
    let token = doc.base();
    let files = doc.files().map_err(|e| format!("files: {e}"))?;
    if files.base != token {
        return Err("files carry another base token".to_string());
    }
    let view = doc
        .view(&ViewSpec::default())
        .map_err(|e| format!("view: {e}"))?;
    if view.base != token {
        return Err("a view carries another base token".to_string());
    }
    let body = files
        .files
        .iter()
        .find(|(n, _)| n == "document.md")
        .map(|(_, t)| t.as_str())
        .unwrap_or_default();
    let named = refs(body);
    let mut targets: Vec<&str> = Vec::new();
    for kind in ['p', 't', 'r'] {
        let of_kind: Vec<&&str> = named.iter().filter(|r| r.starts_with(kind)).collect();
        targets.extend(of_kind.first().copied().copied());
        targets.extend(of_kind.last().copied().copied());
    }
    targets.dedup();
    for target in &targets {
        let inspected = doc
            .inspect(target)
            .map_err(|e| format!("inspect {target}: {e}"))?;
        if inspected.base != token || inspected.items.is_empty() {
            return Err(format!(
                "inspect {target}: an empty answer or another token"
            ));
        }
        let found = doc
            .lookup(target)
            .map_err(|e| format!("lookup {target}: {e}"))?;
        if found.base != token {
            return Err(format!("lookup {target}: another base token"));
        }
    }
    // H13 with nothing pending: no receipt, status 0, and `strict` fails exactly when the report has a violation.
    let report = doc.check();
    for strict in [false, true] {
        let result = doc
            .check_pending(None, None, strict)
            .map_err(|e| format!("check_pending: {e}"))?;
        let want = if strict && report.violations > 0 {
            5
        } else {
            0
        };
        if result.receipt.is_some()
            || result.exit_status != want
            || result.report != report
            || result.strict != strict
        {
            return Err(format!(
                "check_pending with nothing pending and strict {strict} did not say what check says"
            ));
        }
    }
    if doc.check() != report || doc.base() != token {
        return Err("check changed the document or is not repeatable".to_string());
    }
    for listing in ["styles", "numbering", "sections"] {
        let inspected = doc
            .inspect(listing)
            .map_err(|e| format!("inspect {listing}: {e}"))?;
        if inspected.base != token {
            return Err(format!("inspect {listing}: another base token"));
        }
    }
    Ok(())
}
