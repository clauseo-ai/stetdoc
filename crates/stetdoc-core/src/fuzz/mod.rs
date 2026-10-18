//! Entry points for the fuzz targets, behind the off-by-default `fuzzing` feature.
//!
//! **Owns.** The functions that the targets in `fuzz/` call, and nothing else. Each takes the
//! bytes a fuzzer made, drives one module through the calls an edit makes, and aborts when an
//! invariant of that module breaks. `package` and `xml` are private to the crate, so the feature
//! makes this module public and it is the only way in. It is not part of the public interface:
//! without the feature the crate exports exactly what `lib.rs` re-exports, and with it nothing
//! else changes.
//!
//! **Interface.** `package(&[u8])`, `xml(&[u8])`, and the two that go through `Document`: `batch(&[u8])`, a batch
//! parsed and applied, and `files(&[u8])`, an edit of the readable files (`fuzz/document.rs`).
//!
//! **Invariants.**
//! - The first four bytes of an input choose what the harness does, and the rest is the document,
//!   so a fuzzer can change the choice and the document apart. An input shorter than four bytes
//!   does nothing.
//! - Refusing hostile input is never a failure. A panic, a hang, a runaway allocation or a broken
//!   law is. The laws are the container's and the span tree's own: an unedited package or part
//!   writes back its input, an edit changes only what it touches, and what an edit writes can be
//!   read again as the tree that wrote it.
//! - It panics in one place, `fail`, because a panic is how libFuzzer sees a broken law. The
//!   core's no-panic rule is about input, and this module is compiled only for fuzzing.
//! - Nothing in the crate names this module, and it recurses on nothing the input controls.
//!
//! **Depends on.** `api`, `package`, `xml`, `document`.
//!
//! **Start reading.** `fuzz/package_laws.rs` or `fuzz/xml_laws.rs`.

mod document;
mod package_laws;
mod xml_edits;
mod xml_laws;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_document;

use crate::api::error::{ErrorCode, StetError};

/// The container: open the input as a package, read every part, then replace one part and write.
///
/// The first four bytes choose which part is replaced and with what, and the rest is the package.
/// It panics when a law of `Package` breaks.
pub fn package(data: &[u8]) {
    package_laws::run(data);
}

/// The span tree: parse the input as one part, serialize it clean and with every tag dirty, then
/// make one edit.
///
/// The first four bytes choose the edit, and the rest is the part. It panics when a law of
/// `SpanTree` breaks.
pub fn xml(data: &[u8]) {
    xml_laws::run(data);
}

/// A batch: parsed as JSON or block text, written as JSON and parsed again, and applied to a small document.
///
/// The first four bytes choose tracked or untracked, a dry run and the unit of offsets, and the rest is the batch, in which
/// `BASE_TOKEN` stands for the document's base token. It panics
/// when the batch's JSON form parses to something else, or when the edit breaks a law of `Document::edit`: an `internal`
/// error, a failed edit or a dry run that changes the document, or a result that does not save and open again.
pub fn batch(data: &[u8]) {
    let _ = document::batch(data);
}

/// An edit of the readable files: the text goes into `document.md` of a small document, whole, spliced in or appended, and
/// the files are applied as an edit.
///
/// The first four bytes choose how the text goes in and where, and tracked or a dry run. It panics when the edit breaks a law
/// of `Document::edit`, as `batch` does.
pub fn files(data: &[u8]) {
    let _ = document::files(data);
}

/// Aborts the process, which is how libFuzzer sees a broken law.
#[expect(
    clippy::panic,
    reason = "a broken law must abort, and a panic is how the fuzzer sees it"
)]
fn fail(why: &str) -> ! {
    panic!("{why}")
}

/// Splits an input into the four bytes of choices and the document.
fn split(data: &[u8]) -> Option<([u8; 4], &[u8])> {
    let (head, doc) = data.split_first_chunk::<4>()?;
    Some((*head, doc))
}

/// Whether `e` is one of the ways hostile input is refused.
fn is_refusal(e: &StetError) -> bool {
    matches!(
        e.code(),
        ErrorCode::MalformedInput | ErrorCode::LimitExceeded | ErrorCode::Unsupported
    )
}

/// Whether `e` is one of the ways an edit is refused: the payload cannot be written, the change
/// is not supported at that place, or a limit is reached.
fn is_edit_refusal(e: &StetError) -> bool {
    matches!(
        e.code(),
        ErrorCode::InvalidPayload | ErrorCode::Unsupported | ErrorCode::LimitExceeded
    )
}
