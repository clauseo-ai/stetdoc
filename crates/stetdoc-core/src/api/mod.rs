//! The public data types. Data and docs only, no logic.
//!
//! **Owns.** Every type that crosses the core's public interface: the error vocabulary in [`error`], the input bounds
//! in [`limits`], the view types in [`view`], the readable files in [`files`], the answer to a query in [`lookup`],
//! what a check reports in [`check`], and the stamp and the options, the operations with the table
//! that describes them, the batch and the edit, the receipt, the revisions, the inspection, the working copy and its
//! write plan, and the capabilities. They derive `serde` and `schemars`, so the napi binding, `--json` output, the JSON
//! Schema, `capabilities()` and the CLI reference are generated from one source.
//!
//! **Interface.** What `lib.rs` re-exports. Siblings use these types and never redefine them.
//!
//! **Invariants.**
//! - No logic beyond accessors, constructors and the pure grouping of `AuthorRevisions::group`.
//! - A public item without documentation fails the build (`deny(missing_docs)`).
//! - Error codes are frozen from 1.0: new ones may be added, existing ones are never repurposed or removed outside a major
//!   version.
//! - A public type holds strings, numbers, booleans, vectors and `api` types, and nothing from inside the core.
//!
//! **Depends on.** nothing.
//!
//! **Start reading.** `api/error.rs`, then `api/op.rs`.

pub(crate) mod batch;
pub(crate) mod capabilities;
pub(crate) mod check;
pub(crate) mod edit;
pub(crate) mod error;
pub(crate) mod files;
pub(crate) mod inspect;
pub(crate) mod limits;
pub(crate) mod lookup;
pub(crate) mod op;
pub(crate) mod op_spec;
pub(crate) mod options;
pub(crate) mod receipt;
pub(crate) mod revision;
pub(crate) mod stamp;
pub(crate) mod view;
pub(crate) mod working_copy;
