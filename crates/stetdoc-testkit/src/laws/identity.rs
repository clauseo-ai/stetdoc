//! L1 and L4: an unedited document is its input, and a working copy is stable.

use stetdoc_core::{Document, OpenOptions};

/// **L1, identity.** `open` then `save` gives the input bytes exactly, and the same bytes come back through `export`,
/// `resume` and `save`. **L4, stability.** `export`, `resume`, `export` gives the same bytes, and the resumed document has
/// the same base token. A refusal at `open` is the caller's to expect: this takes a document that opens.
pub fn identity(bytes: &[u8]) -> Result<(), String> {
    let options = OpenOptions::default();
    let doc = Document::open(bytes.to_vec(), &options).map_err(|e| format!("open: {e}"))?;
    let saved = doc.save().map_err(|e| format!("save: {e}"))?;
    if saved.bytes != bytes {
        return Err("L1: open then save differs from the input".into());
    }
    if saved.receipt.edited {
        return Err("L1: an unedited document says it was edited".into());
    }
    let working_copy = doc.export().map_err(|e| format!("export: {e}"))?;
    let back = Document::resume(&working_copy, &options).map_err(|e| format!("resume: {e}"))?;
    if back.base() != doc.base() {
        return Err("L4: the resumed document has another base token".into());
    }
    let saved = back.save().map_err(|e| format!("save after resume: {e}"))?;
    if saved.bytes != bytes {
        return Err("L1: open, export, resume, save differs from the input".into());
    }
    if saved.receipt.edited {
        return Err("L1: a resumed unedited document says it was edited".into());
    }
    let again = back.export().map_err(|e| format!("second export: {e}"))?;
    if again != working_copy {
        return Err("L4: export, resume, export is not stable".into());
    }
    Ok(())
}
