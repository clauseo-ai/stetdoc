//! The stetdoc engine.
//!
//! stetdoc projects a `.docx` package into a working copy that an agent can read and
//! edit, and compiles the edits back into the same package with real Word tracked
//! changes (revisions). Everything the edits did not touch is preserved byte for byte.
//! This crate is the one engine behind every surface: the CLI, the napi binding and the
//! text editor backend. It does no file I/O, reads no clock, and is deterministic:
//! bytes in, bytes out.
//!
//! # The public interface
//!
//! The interface is one handle, `Document`, built around the most common loop:
//! `open`, then `view`, then `edit`, then `save`. Alongside it are `resume` and
//! `export` for working copies, `inspect`, `revisions`, `lookup` and `check`, and two
//! free functions, `compare` and `capabilities`. Every failure is a [`StetError`]
//! carrying a stable [`ErrorCode`]. `ARCHITECTURE.md` lists the whole interface and
//! its invariants. Decision 0001 records why it has this shape.
//!
//! **The interface is built in steps** (`docs/design/document-and-ops.md`, section 11). This file re-exports the data types, the
//! handle and the free functions that exist, and a method whose step has not landed returns `unsupported`.
//!
//! # Where things live
//!
//! Everything except the re-exports below is private to the crate. The modules are
//! `api`, `package`, `xml`, `schema`, `wordml`, `styles`, `numbering`, `refs`, `state`,
//! `revisions`, `threads`, `comments`, `index`, `check`, `ops`, `projection`, `compare` and `document`. They
//! may depend on each other only as the Allowed dependencies table in
//! `ARCHITECTURE.md` says, and `cargo xtask check` enforces it. Each module's `mod.rs`
//! opens with its contract.
//!
//! # Fuzzing
//!
//! One module is not private: `fuzz`, which exists only with the off-by-default `fuzzing`
//! feature. It holds the entry points that the fuzz targets in `fuzz/` call, because the
//! modules they test are private. It is not part of the public interface, and a build
//! without the feature does not contain it.
//!
//! # Lints
//!
//! The core never panics on input and does no I/O, and the lints below keep it that way:
//! indexing and slicing that can panic, `unreachable!`, `todo!`, `unimplemented!`,
//! `dbg!`, printing, and `#[allow]` attributes are denied (so are `unwrap`, `expect` and
//! `panic!`, at the workspace level). The only way around one is a local
//! `#[expect(clippy::..., reason = "...")]` with a real reason. Tests are exempt from the
//! panic lints. `cargo xtask check` also rejects `std` file, network, process,
//! environment and clock APIs anywhere in the core.

#![deny(missing_docs)]
#![deny(
    clippy::indexing_slicing,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::allow_attributes,
    clippy::allow_attributes_without_reason
)]

mod api;
#[cfg(test)]
mod bench;
mod check;
mod comments;
mod compare;
mod document;
mod index;
mod numbering;
mod ops;
mod package;
mod projection;
mod refs;
mod revisions;
mod schema;
mod state;
mod styles;
mod threads;
mod wordml;
mod xml;

#[cfg(feature = "fuzzing")]
pub mod fuzz;

pub use api::batch::Batch;
pub use api::capabilities::{Capabilities, ErrorInfo, FieldInfo, FindingInfo, FormatInfo, OpInfo};
pub use api::check::{CheckReport, Class, Finding, FindingCode};
pub use api::edit::Edit;
pub use api::error::{ErrorCode, ExitClass, StetError};
pub use api::files::{NewParagraph, Origin, Place, ReadableFiles, RenderedBlock};
pub use api::inspect::{InspectItem, Inspection};
pub use api::limits::Limits;
pub use api::lookup::{Hit, Lookup, Reference, Site};
pub use api::occurrence::{Occurrence, Occurrences};
pub use api::op::{
    CommentOp, DeleteCommentOp, DeleteNoteOp, DeleteOp, EditCommentOp, FormatOp, InsertOp, ListOp,
    MergeOp, Op, ParagraphOp, PropSet, ReplaceOp, ReplyOp, ResolveOp, ReviewOp, RowAction, RowOp,
    SetOp, SplitOp, StyleOp,
};
pub use api::op_spec::{FieldKind, FieldSpec, OPS, OpSpec};
pub use api::options::{CompareOptions, OffsetUnit, OpenOptions, Quotes};
pub use api::receipt::{
    AuthorRevisions, ChangeEntry, CheckResult, CommentEffect, FoldedEntry, MoveEntry, OpResult,
    Receipt, RemovedOwnEntry, Resolution, ResolvedCount, Retired, Validation,
};
pub use api::revision::{ChangeKind, RevisionInfo};
pub use api::stamp::Stamp;
pub use api::view::{
    Budget, CommentFilter, Cursor, FormatId, Mode, Position, RevisionFilter, Scope, StoryName,
    View, ViewSpec, Warning, WarningCode,
};
pub use api::working_copy::{
    Complement, Recovery, SaveReceipt, Saved, WorkingCopy, WriteOptions, WritePlan, WriteStep,
};
pub use document::{Compared, Document, capabilities, compare, json_schema};
