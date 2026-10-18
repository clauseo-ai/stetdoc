//! Claude's text editor tool over stetdoc's virtual readable files.
//!
//! **Owns.** The backend for the `view`, `str_replace`, `insert` and `create` commands of Claude's text editor tool
//! (`text_editor_20250728`, tool name `str_replace_based_edit_tool`), working on the readable files that
//! `Document::files` renders, as `Document::export` would write them. The design is `docs/design/napi-and-editor.md`,
//! section 5.
//!
//! **Interface.** `Command::parse` reads a `tool_use` input, `run` carries it out on a `&mut Document` with a `Stamp`,
//! and `tool_result` renders the outcome as the text and `is_error` flag of a `tool_result` block; `respond` does all
//! three and returns the receipt of a committed edit. Paths may sit under a prefix, such as `/documents/acme/`. This crate depends only on `stetdoc-core`'s public interface, and the compiler enforces it. It must never reach
//! into core internals. It holds no state: the caller keeps the `Document` (or its `WorkingCopy`) between turns.
//!
//! **Invariants.**
//! - `view` shows a readable file with line numbers, in the mode the format fixes, and `/` lists the files.
//! - `str_replace` requires a unique match. Otherwise it returns `ambiguous_match`, listing each match with context, or
//!   `not_found`.
//! - `str_replace` and `insert` each commit immediately as one tracked `Edit::files`, and return the touched blocks' new
//!   text with their line numbers in the new file.
//! - `create` (and `undo_edit`) return `unsupported`.
//! - A failure never changes the document: the core's edit is atomic, and a failed match makes no edit.
//!
//! **Start reading** at `run.rs`.

#![deny(missing_docs)]

mod command;
mod render;
mod run;

pub use command::Command;
pub use render::tool_result;
pub use run::{Failure, Output, Response, respond, run};
