# 0004. The public interface: what `lib.rs` re-exports

- **Date:** 2026-10-06
- **Status:** accepted on 2026-10-06, with the design in [`docs/design/document-and-ops.md`](../design/document-and-ops.md)

## Context

[Decision 0001](0001-core-interface.md) chose one `Document` handle, an `Edit` builder, a `Stamp` with no default and data types that derive `serde` and `schemars`. `ARCHITECTURE.md` sketched the methods. The work on `Document` built the types and the
design for the rest, and the design found that five of the sketched signatures needed to change. A change to what `lib.rs` re-exports needs a record, so this is it.

## Decision

`lib.rs` re-exports:

- **The handle and the free functions:** `Document`, `Edit`, `compare` (with `Compared`), `capabilities` and `json_schema`.
- **The error vocabulary:** `StetError`, `ErrorCode` and `ExitClass`. `StetError` carries a code, a message, a next step, the index of the operation that failed, and, for an edit of
  readable files, the **file and the line** the error came from.
- **The data types of `api/`:** `Batch`, `Op` and its nineteen payloads (`ReplaceOp`, `InsertOp`, and so on), `OpSpec`, `FieldSpec`, `FieldKind` and `OPS`, `OpenOptions`, `CompareOptions`, `Quotes`, `OffsetUnit`, `Stamp`, `Limits`, the view types
  (`ViewSpec`, `View`, `Mode`, `Budget`, `Cursor`, `Position`, `Scope`, `RevisionFilter`, `CommentFilter`, `StoryName`, `FormatId`, `Warning`, `WarningCode`), `ReadableFiles` and its parts, `Lookup` and its parts, `CheckReport`, `Finding`
  and `FindingCode`, the receipt types, `RevisionInfo` and `ChangeKind`, `Inspection`, `WorkingCopy`, `Complement`, `WriteOptions`, `WritePlan`, `WriteStep`, `Recovery`, `Saved` and `SaveReceipt`, and `Capabilities` with its parts.

Everything else stays `pub(crate)`. The methods are those of design 1.1: thirteen on `Document`, `pending_files`, `ensure_clean`, `write_plan` and `recover` on `WorkingCopy`, and `Batch::{new, parse, to_json, to_block}`.

Five signatures differ from the sketch in `ARCHITECTURE.md`:

1. **Refs are `String`**, not a `Ref` newtype. They are parsed where used, with `invalid_payload` for a malformed one, so the napi and JSON forms need no wrapper. `inspect` takes its target as text too, because `styles`, `numbering` and `sections` sit beside refs.
2. **`revisions`, `export` and `files` return `Result`**, because reading a story and writing a ZIP can fail.
3. **`base`, `files` and `check_pending` are new.** Each is small, and each is used by a surface that would otherwise rebuild it.
4. **`json_schema` is new.** The generated TypeScript types and the CLI reference read the schema from the binary, because `xtask` is built without the core.
5. **`Batch::to_block` returns `Result`**, because block text cannot say a newline inside a list entry, a carriage return or a property with children. JSON says all of it.

The wire conventions are snake_case fields, hyphenated operation names (`edit-comment`) and snake_case codes. `ErrorCode`, `WarningCode` and `FindingCode` serialize as their wire names. The grammar of block text is provisional, confined to
`ops/parse/block.rs` and to `FieldKind`, until experiments show which syntax agents write most reliably.

## Consequences

- The types in `api/files.rs`, `api/lookup.rs`, `api/view.rs` and `api/check.rs`, first written with the modules that fill them, are part of this interface from this record on.
- Adding an operation is a payload and a variant in `api/op.rs`, an entry in `api/op_spec.rs`, and a file in `ops/`. The parsers, `capabilities()`, `docs/reference/operations.md` and the conformance suite follow from the table.
- `serde`, `serde_json` and `schemars` join `stetdoc-core`. They do no I/O, and the layering check still holds.
- Error codes are frozen from 1.0, and so is this list for a major version: a type may gain a field and an enum is `#[non_exhaustive]` where it can grow, but nothing is removed or renamed.
