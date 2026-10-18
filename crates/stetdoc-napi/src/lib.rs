//! Node-API bindings for stetdoc, for Node and Bun.
//!
//! **Owns.** Thin bindings for `Document`, `compare`, `capabilities`, `json_schema` and the editor, plus JSON
//! marshalling. The package's JavaScript in `npm/` builds the public TypeScript surface on them
//! (`docs/design/napi-and-editor.md`). TypeScript types are generated from the core's `serde` and `schemars`
//! definitions, so they cannot drift from Rust.
//!
//! **Interface.** One class, `NativeDocument`, and the functions `capabilities` and `jsonSchema`. Every data type
//! crosses as a JSON string made by the core's `serde`, and bytes cross as `Buffer`. A `StetError` is thrown as a
//! JavaScript `Error` whose message is `stetdoc:` followed by the error's JSON, which `npm/index.js` turns into a
//! `StetdocError`.
//!
//! **Invariants.**
//! - This is the only crate allowed to use `unsafe`, because Node-API is a C interface. Its lints are a copy of the
//!   workspace table with that one change. The unsafe code is the one `napi-derive` generates; none is written here.
//! - Calls are synchronous: bytes in, bytes out. Synchronous Node-API calls are the safest subset under Bun.
//! - Every call runs under `catch_unwind`, so a panic is the error `internal` and never a crashed process.
//! - No file I/O and no clock reads. The caller supplies the author and the date.
//!
//! **Start reading** at `native.rs`, then `marshal.rs`.

#![deny(missing_docs)]

mod marshal;
mod native;

// The addon's exports are registered by `napi-derive` and not reached from Rust, so they are re-exported here, which also
// documents them.
pub use native::{
    NativeDocument, NativeEntry, NativeSaved, NativeToolResult, NativeWorkingCopy, capabilities,
    json_schema,
};
