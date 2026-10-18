//! The targets that go through `Document` (design `document-and-ops.md` 9.4): a batch, parsed and applied to a small document,
//! and an edit of that document's readable files.
//!
//! Each input is four bytes of choices and the text. For a batch the text is JSON or block text, and the choices pick tracked or
//! untracked, a dry run, and the unit of offsets. Every `BASE_TOKEN` in the text is replaced by the document's base token before
//! the batch is parsed, since a fuzzer cannot guess one and `Batch::parse` refuses a token that is not one. For an edit of files the text goes into `document.md`, whole, spliced in, or appended, as the choices say.
//!
//! A refusal is never a failure. A panic, a hang, an `internal` error, a failed edit that changes the document, a dry run that
//! changes it, an edit whose result does not save and open again, and a parsed batch whose JSON form does not parse back to it,
//! are.

use super::{fail, split};
use crate::api::batch::Batch;
use crate::api::edit::Edit;
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;
use crate::api::options::{OffsetUnit, OpenOptions};
use crate::api::stamp::Stamp;
use crate::document::Document;

/// The document both targets edit: a few paragraphs with quotes, a tab and an emoji, a table, and a paragraph after it.
const BODY: &str = concat!(
    "<w:p><w:r><w:t>The Supplier shall indemnify the Customer, and shall pay on demand.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t>\u{201C}Losses\u{201D} means any loss, and the Supplier\u{2019}s liability.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t xml:space=\"preserve\">Emoji \u{1F600} then a tab</w:t></w:r><w:r><w:tab/></w:r><w:r><w:t>and the end.</w:t></w:r></w:p>",
    "<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p>",
    "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>",
    "<w:tr><w:tc><w:p><w:r><w:t>a1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>b1</w:t></w:r></w:p></w:tc></w:tr>",
    "</w:tbl>",
    "<w:p><w:r><w:t>After the table.</w:t></w:r></w:p>",
);

const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The CRC-32 of ZIP, bit by bit: the document is small and built once per input.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP of stored parts.
fn stored_zip(parts: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let le16 = |n: usize| u16::try_from(n).unwrap_or(u16::MAX).to_le_bytes();
    let le32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes();
    for (name, data) in parts {
        let offset = out.len();
        let crc = crc32(data).to_le_bytes();
        let name = name.as_bytes();
        out.extend(b"PK\x03\x04");
        out.extend([20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        out.extend(crc);
        out.extend(le32(data.len()));
        out.extend(le32(data.len()));
        out.extend(le16(name.len()));
        out.extend([0, 0]);
        out.extend(name);
        out.extend(data);
        central.extend(b"PK\x01\x02");
        central.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0]);
        central.extend(crc);
        central.extend(le32(data.len()));
        central.extend(le32(data.len()));
        central.extend(le16(name.len()));
        central.extend([0u8; 12]);
        central.extend(le32(offset));
        central.extend(name);
    }
    let start = out.len();
    out.extend(&central);
    out.extend(b"PK\x05\x06");
    out.extend([0, 0, 0, 0]);
    out.extend(le16(parts.len()));
    out.extend(le16(parts.len()));
    out.extend(le32(central.len()));
    out.extend(le32(start));
    out.extend([0, 0]);
    out
}

/// The document's package.
pub(super) fn docx() -> Vec<u8> {
    let decl = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
    let types = format!(
        "{decl}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>"
    );
    let rels = format!(
        "{decl}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>"
    );
    let document = format!(
        "{decl}<w:document xmlns:w=\"{NS_W}\"><w:body>{BODY}<w:sectPr/></w:body></w:document>"
    );
    stored_zip(&[
        ("[Content_Types].xml", types.into_bytes()),
        ("_rels/.rels", rels.into_bytes()),
        ("word/document.xml", document.into_bytes()),
    ])
}

fn stamp() -> Stamp {
    Stamp::new("Fuzz", "2026-10-06T10:00:00")
}

/// Opens the document, which must open.
fn opened() -> Document {
    match Document::open(docx(), &OpenOptions::default()) {
        Ok(doc) => doc,
        Err(e) => fail(&format!("the fuzz document does not open: {e}")),
    }
}

/// What a document is: its base token and the bytes it saves as.
fn state_of(doc: &Document) -> (String, Vec<u8>) {
    match doc.save() {
        Ok(saved) => (doc.base(), saved.bytes),
        Err(e) => fail(&format!("a document that opened does not save: {e}")),
    }
}

/// What became of an input, for the tests: it was not read as one, its edit was refused, or its edit applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    /// Too short for the choices, not UTF-8, or not a batch.
    Skipped,
    /// The edit was refused, and the document is as it was.
    Refused,
    /// The edit applied (or its dry run did), and the laws held.
    Applied,
}

/// Applies an edit and holds it to the laws of the module's contract.
fn judge(mut doc: Document, edit: Edit, dry_run: bool) -> Outcome {
    let before = state_of(&doc);
    match doc.edit(edit) {
        Ok(_) => {
            if dry_run {
                if state_of(&doc) != before {
                    fail("a dry run changed the document");
                }
                return Outcome::Applied;
            }
            let (_, bytes) = state_of(&doc);
            if let Err(e) = Document::open(bytes, &OpenOptions::default()) {
                fail(&format!("what an edit saved does not open again: {e}"));
            }
            Outcome::Applied
        }
        Err(e) => {
            if e.code() == ErrorCode::Internal {
                fail(&format!("an edit failed with internal: {e}"));
            }
            if state_of(&doc) != before {
                fail("a failed edit changed the document");
            }
            Outcome::Refused
        }
    }
}

/// The placeholder for the document's base token in a batch.
pub(super) const BASE: &str = "BASE_TOKEN";

/// A batch: parsed with the document's base token in place of `BASE`, written as JSON and parsed again, then applied.
pub(super) fn batch(data: &[u8]) -> Outcome {
    let Some((choices, text)) = split(data) else {
        return Outcome::Skipped;
    };
    let Ok(text) = std::str::from_utf8(text) else {
        return Outcome::Skipped;
    };
    let doc = opened();
    let text = text.replace(BASE, &doc.base());
    let limits = Limits::default();
    let batch = match Batch::parse(&text, &limits) {
        Ok(batch) => batch,
        Err(e) => {
            if !matches!(
                e.code(),
                ErrorCode::InvalidPayload | ErrorCode::LimitExceeded
            ) {
                fail(&format!(
                    "a batch was refused with {}, not invalid_payload or limit_exceeded",
                    e.code().as_str()
                ));
            }
            return Outcome::Skipped;
        }
    };
    match Batch::parse(&batch.to_json(), &limits) {
        Ok(again) if again == batch => {}
        Ok(_) => fail("a batch written as JSON parses to another batch"),
        Err(e) => fail(&format!("a batch written as JSON does not parse: {e}")),
    }
    let [mode, _, _, _] = choices;
    let mut edit = Edit::ops(batch, stamp());
    if mode & 1 == 1 {
        edit = edit.untracked();
    }
    if mode & 4 == 4 {
        edit = edit.offsets(OffsetUnit::Utf16);
    }
    let dry_run = mode & 2 == 2;
    if dry_run {
        edit = edit.dry_run();
    }
    judge(doc, edit, dry_run)
}

/// The largest index at or below `at` that is the start of a character of `text`.
fn floor_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// An edit of the readable files: the text goes into `document.md`, and the files are applied as an edit.
pub(super) fn files(data: &[u8]) -> Outcome {
    let Some((choices, text)) = split(data) else {
        return Outcome::Skipped;
    };
    let Ok(text) = std::str::from_utf8(text) else {
        return Outcome::Skipped;
    };
    let doc = opened();
    let mut files = match doc.files() {
        Ok(files) => files,
        Err(e) => fail(&format!("the fuzz document has no readable files: {e}")),
    };
    let Some((_, md)) = files
        .files
        .iter_mut()
        .find(|(name, _)| name == "document.md")
    else {
        fail("the readable files have no document.md");
    };
    let [how, hi, lo, mode] = choices;
    match how % 3 {
        0 => *md = text.to_string(),
        1 => {
            let at = floor_boundary(md, usize::from(u16::from_be_bytes([hi, lo])));
            md.insert_str(at, text);
        }
        _ => md.push_str(text),
    }
    let mut edit = Edit::files(files, stamp());
    if mode & 1 == 1 {
        edit = edit.untracked();
    }
    let dry_run = mode & 2 == 2;
    if dry_run {
        edit = edit.dry_run();
    }
    judge(doc, edit, dry_run)
}
