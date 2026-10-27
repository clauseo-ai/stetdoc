//! The receipt of a command that writes a .docx (`edit -o`, `edit --in-place`, `compare`), told for the written file.
//!
//! A saved file opens in a new ref namespace (`GLOSSARY.md`, Ref namespace), so the base the edit ended at is not valid for
//! it. Opening the written bytes gives the base that the next open of that file will have, but refs are assigned again in
//! document order there, so the receipt's refs hold only when the edit added and retired no block. The two documents' readable
//! files say whether they do: when they are the same apart from the base, the receipt carries the written file's base and an
//! agent can chain the next batch from it; otherwise it carries no base, and `next` says to view the written file.

use crate::output::{Failure, to_json};
use std::path::Path;
use stetdoc_core::{Document, OpenOptions, Receipt};

/// The text and the JSON of a receipt, for the file it wrote.
pub struct Written {
    /// The receipt's text, its head line holding the written file's base or none, then `wrote` and any `next`.
    pub text: String,
    /// The receipt as JSON, with `base` and `text` for the written file. `base` is null when the refs changed.
    pub receipt: serde_json::Value,
    /// The `next` step, when there is one.
    pub next: Option<String>,
}

/// Retells `receipt`, which `session` produced, for `bytes` saved at `path`. `hint` is a `next` step to add even when the
/// base carries over, such as listing the revisions of a redline.
pub fn retell(
    session: &Document,
    receipt: &Receipt,
    bytes: &[u8],
    path: &Path,
    hint: Option<String>,
) -> Result<Written, Failure> {
    let reopened = Document::open(bytes.to_vec(), &OpenOptions::default())?;
    let kept = same_refs(session, &reopened)?;
    let old = format!(". base {}", receipt.base);
    let mut json = to_json(receipt)?;
    let (head, next) = if kept {
        let base = reopened.base();
        json["base"] = serde_json::Value::String(base.clone());
        (
            receipt.text.replacen(&old, &format!(". base {base}"), 1),
            hint,
        )
    } else {
        json["base"] = serde_json::Value::Null;
        let why = "the written file numbers its blocks again, so take the next base from a view";
        let view = hint.unwrap_or_else(|| format!("stetdoc view {}", path.display()));
        (
            receipt.text.replacen(&old, ".", 1),
            Some(format!("{view} ({why})")),
        )
    };
    json["text"] = serde_json::Value::String(head.clone());
    let mut text = format!("{}\nwrote {}\n", head.trim_end(), path.display());
    if let Some(next) = &next {
        text.push_str(&format!("next: {next}\n"));
    }
    Ok(Written {
        text,
        receipt: json,
        next,
    })
}

/// Whether two documents give every block the same ref: their readable files are the same once each one's base is taken out.
fn same_refs(a: &Document, b: &Document) -> Result<bool, Failure> {
    let (fa, fb) = (a.files()?, b.files()?);
    if fa.files.len() != fb.files.len() {
        return Ok(false);
    }
    Ok(fa.files.iter().zip(&fb.files).all(|((na, ta), (nb, tb))| {
        na == nb && ta.replace(&fa.base, "") == tb.replace(&fb.base, "")
    }))
}
