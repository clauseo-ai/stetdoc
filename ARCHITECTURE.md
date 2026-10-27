# Architecture

**Status:** accepted on 2026-10-05. The decision
behind it is recorded in [0001](docs/decisions/0001-core-interface.md).

This document is the map for anyone new to the code, human or agent. Read it after
`README.md` and before changing anything. Domain terms are defined in
[`GLOSSARY.md`](GLOSSARY.md). Architecture terms follow common usage: a module is a unit of code behind an
interface, a seam is a place where behaviour can vary without editing the caller, an adapter
fills a seam, a deep module hides much behind a small interface (John Ousterhout, *A Philosophy
of Software Design*), leverage is what a caller gets for each thing it must learn, and locality means a change stays in one place.

## Bird's-eye view

stetdoc turns a `.docx` **package** into a **projection** an agent can read and edit,
and turns edits back into the same package with **revisions** (Word tracked changes).
Everything the edits didn't touch is preserved byte for byte.

There is one engine, `stetdoc-core`, behind every surface:

```
      agents (OpenCode, cmx2, Claude Code, …)              host applications
                  │                                  ┌──────────┴───────────┐
          stetdoc-cli (binary)               stetdoc-napi (Node/Bun)   stetdoc-editor
                  └──────────────┬───────────────────┘   (Claude's text editor tool)
                           stetdoc-core
                (no I/O, deterministic, bytes in, bytes out)
```

The public interface is one handle, `Document`, built around the most common loop:

```rust
let mut doc = Document::open(bytes, &OpenOptions::default())?;  // or Document::resume(&working_copy, &options)
let view = doc.view(&ViewSpec::default())?;                      // outline or text, sized to a budget
let receipt = doc.edit(Edit::ops(batch, stamp))?;                // validated, atomic, tracked by default
let saved = doc.save()?;                                         // serializes; unedited ⇒ input bytes
```

## Public interface

`crates/stetdoc-core/src/lib.rs` re-exports exactly this, the data types of `api/` and the error vocabulary
(`StetError`, `ErrorCode` and `ExitClass`). Everything else is `pub(crate)`. [Decision 0004](docs/decisions/0004-public-interface.md)
records the list, and `docs/design/document-and-ops.md` says what each part does.

```rust
impl Document {
    pub fn open(bytes: Vec<u8>, options: &OpenOptions) -> Result<Document, StetError>;
    pub fn resume(working_copy: &WorkingCopy, options: &OpenOptions) -> Result<Document, StetError>;

    pub fn base(&self) -> String;                                  // the base token a batch must name

    pub fn view(&self, spec: &ViewSpec) -> Result<View, StetError>;
    pub fn edit(&mut self, edit: Edit) -> Result<Receipt, StetError>;
    pub fn save(&self) -> Result<Saved, StetError>;

    pub fn inspect(&self, target: &str) -> Result<Inspection, StetError>;   // "p42", "styles", "numbering", "sections"
    pub fn revisions(&self, filter: &RevisionFilter) -> Result<Vec<RevisionInfo>, StetError>;
    pub fn lookup(&self, query: &str) -> Result<Lookup, StetError>;   // defined terms, cross-references
    pub fn occurrences(&self, needle: &str, scope: &Scope) -> Result<Occurrences, StetError>;   // where a text is, as `find` matches
    pub fn occurrences_ignoring_case(&self, needle: &str, scope: &Scope) -> Result<Occurrences, StetError>;   // the same, any case
    pub fn check(&self) -> CheckReport;
    pub fn check_pending(&self, pending: Option<ReadableFiles>, stamp: Option<Stamp>, strict: bool)
        -> Result<CheckResult, StetError>;                       // everything `stetdoc check` prints, and its exit status
    pub fn files(&self) -> Result<ReadableFiles, StetError>;      // the readable files, with no ZIP write
    pub fn export(&self) -> Result<WorkingCopy, StetError>;
}

impl WorkingCopy {
    pub fn pending_files(&self) -> Result<Option<ReadableFiles>, StetError>;
    pub fn ensure_clean(&self) -> Result<(), StetError>;          // `pending_edits`
    pub fn write_plan(&self, disk: Option<&WorkingCopy>, options: &WriteOptions) -> Result<WritePlan, StetError>;
    pub fn recover(disk: &WorkingCopy, marker: &[u8]) -> Result<Recovery, StetError>;
}

impl Batch {
    pub fn parse(text: &str, limits: &Limits) -> Result<Batch, StetError>;   // JSON or block text
    pub fn to_json(&self) -> String;
    pub fn to_block(&self) -> Result<String, StetError>;
}

pub fn compare(original: Vec<u8>, revised: Vec<u8>, options: &CompareOptions)
    -> Result<Compared, StetError>;                    // Compared { document, receipt }
pub fn capabilities() -> Capabilities;
pub fn json_schema() -> String;                        // one JSON Schema document for every public type

impl Edit {
    pub fn ops(batch: Batch, stamp: Stamp) -> Edit;               // operations
    pub fn files(files: ReadableFiles, stamp: Stamp) -> Edit;     // hand-edited readable files
    pub fn untracked(self) -> Edit;
    pub fn scope(self, refs: Vec<String>) -> Edit;                // scope guard: refs and ranges such as "p40..p60"
    pub fn dry_run(self) -> Edit;
    pub fn quotes(self, quotes: Quotes) -> Edit;                  // follow the paragraph's quotes (default) or keep them as typed
    pub fn offsets(self, unit: OffsetUnit) -> Edit;               // scalar values (default) or UTF-16 units
}
```

`ErrorCode` and `StetError` are defined under Errors below. `ExitClass` is the exit status
class of a code (`number()` and `label()`), public so the CLI maps its exit statuses from
the same table as everyone else.

`OpenOptions` carries the input `Limits` and the format version, and has a default.
`CompareOptions` carries limits and a **required** `Stamp`.

### Invariants callers can rely on

**Edits are validated, then committed.**
- `edit` validates the parts it dirtied before committing, under the **no new
  violations** rule:
  - An edit may not add schema or Word-rule violations.
  - Problems already present in the input are reported by `check`, not blocked.
- `edit` is atomic. On an error or a dry run, the `Document` is unchanged.
- A `Document` can therefore always be saved. `save` only serializes, and an unedited
  `Document` returns the input bytes exactly (law L1).

**Tracked by default.** Untracked editing needs an explicit `.untracked()`. A `Stamp`
(author and date) has no default, so it can't be forgotten.

**Refs are stable within a namespace.**
- Refs are assigned in document order at `open`, and every `open` starts a **ref
  namespace**.
  - The namespace id is derived from the whole opened package, so the same bytes always
    give the same namespace.
  - A namespace survives edits and `export`/`resume`. Reopening *saved* bytes starts a
    new one.
- A ref follows its block's element. A block that an edit adds gets its ref when the
  edit commits, never before, so a dry run never issues a ref. A batch is written blind,
  so no operation can name a block created earlier in the same batch.
- **The base token.** Every `View`, `Receipt` and set of readable files carries the
  document's **base token**: its namespace, its **state version** and its **lineage**,
  a hash chain over every committed state of the namespace. An operation batch names the
  base token it was written against, in working copies too. Only `inspect` and
  `Edit::files`, which check their own base, may leave it out.
- **Resolution rules:**
  - A block removed from the document returns `stale_ref`: an untracked deletion, an
    accepted deletion, a rejected insertion, or the absorbed block of a merge. A
    tracked-deleted block keeps its ref until the deletion is accepted, and an
    operation that would edit its text returns `readonly_region`.
  - A ref that never existed returns `not_found`. A malformed ref returns
    `invalid_payload`.
  - A batch whose base token names a different namespace, or a history this document
    does not have (another working copy, or another session opened from the same file),
    returns `stale_ref`, so an old `p42` can never silently hit a different paragraph.
  - A batch that names a block changed after the version it was written against returns
    `stale_ref`. Blocks untouched since stay safe to edit.
- For multi-turn work, callers keep the `Document` alive or keep its `WorkingCopy`.
  They don't reopen saved bytes.

**Readable files carry their base.** `ReadableFiles` records the base token it was
rendered from. Files from any other base (a different namespace or history, or an older
or newer **state version**) return `stale_files`.

**Receipts show the result.** A `Receipt` carries the post-edit text of every block it
touched, the refs of the blocks it added (a dry run lists a new block without a ref), the
new base token, and warnings.

**Determinism.** The same inputs (bytes, edits, stamps, options) always give
byte-identical `save`, `export` and `compare` output. New identifiers come from
inputs, never from a clock or random number generator:
- Revision and comment `w:id`: the highest existing id plus one.
- New `w14:paraId` and `w16cid` durable ids: a hash of the namespace,
  state version and position (the part, and the ordinal of the request), bumped on
  collision. A new paragraph's `w14:textId` is the constant `77777777`, which Word itself
  writes on most paragraphs.
- A revision's `w:date` is the wall-clock time of the date the caller passes, validated and
  never converted.
- The namespace id is a hash of the opened package, and the lineage is a hash chain over
  the committed changes.
- Timestamps of new ZIP entries: copied from the main document part's entry.
- No `w:rsid*` attributes are added.

**Errors.** Every failure is a `StetError`, carrying:
- a stable `ErrorCode`
- a message
- a `next` hint
- the index of the failing operation

The core never panics on input; fuzzing enforces this.

**Limits apply everywhere.** They cover `open`, `resume`, `compare`, readable files and
batches.

**No file, network or clock access.**

**Concurrency.** `Document` is `Send`, not `Sync`: keep one `Document` per file and
serialize edits to it. The napi binding is synchronous, because synchronous
Node-API calls are the safest subset under Bun. A worst-case call on a 300-page
document takes about a second, so hosts that care about their event loop call it from
a worker.

All public data types live in `api/`. They derive `serde` and `schemars`, so the napi
binding, `--json` output, the JSON Schema, `capabilities()` and the CLI reference are
generated from one source and can't drift apart. `api/` holds only the crate's public
interface; internal types shared between modules live with their owner module. `PartKind`,
which names the parts the schema covers, is one of those: it is generated with the tables and
stays `pub(crate)` in `schema`.

### Hand edits and the working copy

Hand edits to readable files enter only through `Edit::files`. The rules for turning
edited files into operations, applied per readable file:

1. **Blocks are aligned by ref, never by position.**
2. Text changed inside a block becomes word-level `replace` operations.
3. A block whose ref line was removed becomes a `delete`.
4. New lines without a ref, after a block, become an `insert` of new paragraphs, which
   take the previous block's style.
5. A duplicated ref returns `invalid_payload`. Reordered refs return `unsupported`:
   moves go through operations.
6. Changing read-only content (clause numbers, field results, someone else's
   deletion, placeholders) returns `readonly_region`.

`resume` restores the state at the last `export`. `WorkingCopy::pending_files()` lists
readable files that differ from what was written.

### Complement format and hostile input

**The complement is versioned.** It holds:
- the complement version and format version
- the base token, and the SHA-256 of the head package bytes and of the ref map
- the original package bytes, and the head package bytes once edited
- the ref map: the namespace and how it was derived, the block-scan and fingerprint
  versions, the lineage chain, each live block's ref with its story, its ordinal in the
  story's block scan, a fingerprint of its text and the version it last changed in, and
  the refs that were retired
- the hashes of the readable files as written

**The agent can write to it, so `resume` trusts nothing.**
- It checks the two SHA-256 digests first.
- It re-parses the head package, rescans every story and requires the ref map to align
  with the scan exactly: the same stories, block counts, kinds and fingerprints.
- It rejects a mismatch or a malformed ref map with `corrupt_complement`. A ref map
  written with another block-scan or fingerprint version than this release's returns
  `unsupported_version`.

**Compatibility.** A release reads complements of its own major version, upgrading
older minor versions on read. Newer complements return `unsupported_version`.

## Code map

```
crates/
  stetdoc-core/src/
    lib.rs          the public interface (re-exports only)
    api/            public data types: op (and `op_spec`, the table that describes every operation),
                    batch, edit, view, files, lookup, check, receipt, revision, inspect, working_copy,
                    error (ErrorCode), stamp, options, limits, capabilities. Data and docs only, no logic
    package/        ZIP: lazy part access, raw copy of untouched entries, content types,
                    relationships, input limits
    xml/            span tree: parse, nodes with byte spans, dirty tracking, serializer, namespaces
    schema/         generated/ (by `cargo xtask codegen`): element order, value types; insertion
                    order and the validator
    wordml/         typed views over span trees: stories and blocks, paragraphs, runs, tables,
                    sections, notes, comments, fields, content controls, anchors, revisions,
                    style and numbering *definitions*; text_model (characters ↔ XML, in
                    three modes) with its map and its field index
    styles/         style cascade, effective formatting, hidden-text rules
    numbering/      clause numbers (pure computation over definitions and styles)
    refs/           the ref table: block ↔ ref, namespaces, tombstones, persistence
    state/          committed State and the copy-on-write Tx; derived-cache invalidation
    revisions/      stamping, tracked text edits, *PrChange, paragraph marks, list
                    instances, accept/reject
    comments/       writing comments: add, reply, resolve, edit, delete, the cleanup of comments whose text
                    went, and the deletion of notes
    threads/        comments and their threads, read from the comment parts and the stories; the audit of
                    how consistent those parts are. Read only
    index/          defined terms, cross-references, reference graph
    check/          validation rules (no new violations) and check reports
    ops/            one file per operation (validate + compile); dispatch; touched-block
                    tracking; batch parsers (JSON and block text, generated from operation specs)
    projection/     model (the neutral tree between engine and formats); view (scope, mode,
                    paging, budget); formats/ (the Format seam); files (readable files ↔ batch)
    compare/        block alignment, diff into revisions, import (cross-package transplant)
    document/       Document and Edit: orchestration (stage → ops → check → scope guard →
                    commit), export/resume, the public compare entry
    fuzz/           only with the off-by-default `fuzzing` feature: the entry points the fuzz
                    targets call, because `package` and `xml` are private. Not public interface
    bench/          test builds only: the benchmarks of the performance budgets, one small file each, registered
                    here and run by the harness in stetdoc-testkit (`cargo xtask bench`)
  stetdoc-editor/   Claude's text editor tool over virtual readable files: command.rs (the tool's
                    input), run.rs (each command, paths under a prefix), render.rs (what the model
                    reads). Depends only on stetdoc-core's public interface; the compiler enforces it
  stetdoc-testkit/  corpus loaders, compare pairs and the redline noise gate of compare, fixture builder,
                    batch generators (from public view output), normalization, conformance
                    suites, law checks, oracle runners (LibreOffice, Word)
  stetdoc-cli/      args (cli.rs); one file per command in commands/; fsio (the only file I/O),
                    with workdir.rs executing the write plan under the lock; clock; the embedded
                    skill (skill/, skill.rs); doc/ holds the CLI reference's tested examples
  stetdoc-napi/     thin bindings for Document, compare, capabilities and the editor (native.rs);
                    JSON marshalling and thrown errors (marshal.rs); tests/types.rs generates
                    npm/types.d.ts. npm/ is the package `@stetdoc/node`: the wrapper, its types, the
                    smoke test, and one package per target under targets/
xtask/              check, docs, codegen, corpus fetch, fuzz, release (`cargo xtask`)
fuzz/               the fuzz targets and their dictionaries: a cargo workspace of its own, built
                    with a pinned nightly toolchain by `cargo xtask fuzz`
skills/stetdoc/     thin SKILL.md stub; the full skill is embedded in the binary
corpus/             the corpus manifest (no documents); `SOURCES.md` records each source and licence
docs/               user guides (agents.md, self-hosting-mcp.md, build-on-stetdoc.md), design/,
                    decisions/, reference/ (generated)
```

### Module contracts

Each module opens with a `//!` comment restating its contract. This table is the index.
The check requires each contract's "Depends on" to match the table below, and each "Start
reading" file to exist.

| Module | Owns | Interface to siblings | Start reading |
|---|---|---|---|
| `api` | The public data types and the error vocabulary | `StetError`, `ErrorCode`, `ExitClass`, and the types of the public interface | `api/error.rs` |
| `package` | ZIP container, part access, package-level byte preservation | `Package::{open, part, replace_part, add_part, add_external_relationship, opened_len, write}` | `package/mod.rs` |
| `xml` | Span tree; XML-level byte identity | `SpanTree::{parse, root, edit, serialize}` | `xml/tree.rs` |
| `schema` | Word's element order and value types (generated), where a new child goes, and the violation set | `Ctx::order_of`, `ctx_at`, `insert_in_order`, `reorder`, `validate_part`, `validate_subtree`, `validate_unit`, `unit_root`, `walk_unit`, `read_children`, `is_unit_boundary`, `in_taken_branch`, `taken_branch`, `disposition`, `PartKind::from_relationship_type` | `schema/mod.rs` |
| `wordml` | What WordprocessingML means: stories, blocks, definitions, the text model and its character-to-XML map | `Story::{scan, refresh, paragraph, block, sections, anchors, revisions, num_facts, table_site}`, `BlockKey`, `stories`, `Rels`, `Paragraph::{text, map}`, `TextMap::{locate, gap, offset_of, find_all}`, `Joined`, `StyleDefs`, `NumberingDefs` | `wordml/text_model.rs` |
| `styles` | The cascade and hidden-text rules | `Cascade::{effective, hidden, looks_hidden}` | `styles/cascade.rs` |
| `numbering` | Clause numbers | `StoryNumbers::{build, clause_number, full_context, notes_at}` | `numbering/counters.rs` |
| `refs` | Ref allocation, namespaces, base tokens, stability | `RefTable::{resolve, allocate, tombstone, rebind, merge, persist, restore}`, `BaseToken` | `refs/table.rs` |
| `state` | Committed state, staging and derived caches | `State`, `PartsView`, `Tx::{part_mut, add_relationship, touch, retire, rebind, merge, commit, discard}`, `State::{cascade, numbers, opened_bytes, opened_source}`, `PartsView::{content_type, cascade, numbers, sites, site, cascade_deps, numbers_deps}`, `SiteHandle` | `state/mod.rs` |
| `revisions` | Every revision shape Word writes; accept and reject | `Compiler::{replace_text, set_props, insert_block, delete_block, split_block, merge_blocks, insert_row, place_markers, …}`, `resolve(tx, &mut ChangeIndex, &[Step])`, `changes` | `revisions/mod.rs` |
| `comments` | Writing comments and deleting notes | `add`, `reply`, `set_resolved`, `edit`, `delete`, `reconcile`, `delete_note` | `comments/add.rs` |
| `threads` | Comments, threads, anchors and the consistency of the comment parts | `Threads::{of, comments, threads, comment, thread_of, covered_text, counts}`, `audit` | `threads/read.rs` |
| `index` | Clauses, defined terms and cross-references | `PartsView::{doc_index, edges, bookmarks}`, `Index::{clauses, clause_of, resolve_clause, definitions}`, `Edges::{outbound, inbound}`, `lookup` | `index/mod.rs` |
| `check` | Validity under the no-new-violations rule, and the findings of a check | `edit(&Tx, &Declared) -> Result<Verified, StetError>`, `document(&PartsView) -> CheckReport`, `scan_unit(&SpanTree, NodeId)`, `scan_part(&SpanTree)`, `unit_keys(&PartsView, &BlockKey)` | `check/mod.rs` |
| `ops` | Operations into compiler calls, and the two syntaxes of a batch | `apply(&Batch, &mut Tx) -> Result<Applied, StetError>`, `Batch::{parse, to_json, to_block}`, `validate::batch`, the quote policy | `ops/mod.rs`, then any `ops/<name>.rs` |
| `projection` | Rendering, selection, paging, parsing edited files | `view(&State, &ViewSpec) -> View`, `render_files`, `render_blocks`, `lookup`, `render_lookup`, `files::to_batch(&State, &ReadableFiles, &Limits) -> Planned` | `projection/model.rs` |
| `compare` | Original and revised into a redline state | `prepare(original, revised, &Limits, &RevStamp) -> Prepared`, `redline(&Prepared, &RevStamp) -> Redline` (the open transaction and a `CompareReceipt`, which `document` checks with the foreign origins and commits) | `compare/mod.rs` |
| `document` | The public handle and orchestration, the working copy and how it is written | the public interface, `WorkingCopy::{pending_files, write_plan, recover}` | `document/mod.rs` |
| `fuzz` | The entry points of the fuzz targets, behind the `fuzzing` feature | `package(&[u8])`, `xml(&[u8])`, `batch(&[u8])`, `files(&[u8])` | `fuzz/package_laws.rs` |
| `bench` | The benchmarks of the performance budgets, in test builds only | none: the ignored test `bench::g6` | `bench/package.rs` |

### Allowed dependencies

`cargo xtask check` reads this table and fails on any `crate::` path that isn't allowed.
It finds paths in `use` statements, grouped uses, inline paths and macro bodies, and
resolves `super::` and `self::` paths to the module they name. It rejects aliases and
globs of the crate root (`use crate as c`, `use crate::*`, `extern crate self`), because they
would hide paths from the check, and it requires `lib.rs` to hold only crate docs,
attributes, `mod x;`, `pub use` and one public module, `fuzz`, behind the `fuzzing` feature.
It also fails when a module's `//!` contract lists different dependencies from this table, or
when the table and the module directories disagree. "Any of the above" means every module in
a row above it. Change the table and the code together.

| Module | May depend on |
|---|---|
| `api` | nothing |
| `package` | `api`, `xml` |
| `xml` | `api` |
| `schema` | `api`, `xml` |
| `wordml` | `api`, `xml`, `schema` |
| `styles` | `api`, `xml`, `wordml` |
| `numbering` | `api`, `wordml`, `styles` |
| `refs` | `api`, `wordml` |
| `state` | `api`, `package`, `xml`, `wordml`, `refs`, `styles`, `numbering` |
| `revisions` | `api`, `xml`, `schema`, `wordml`, `state`, `check` |
| `comments` | `api`, `package`, `xml`, `schema`, `wordml`, `state`, `revisions`, `threads` |
| `threads` | `api`, `xml`, `schema`, `wordml`, `state` |
| `index` | `api`, `wordml`, `styles`, `numbering`, `state` |
| `check` | `api`, `package`, `xml`, `schema`, `wordml`, `state`, `threads` |
| `ops` | `api`, `schema`, `wordml`, `styles`, `refs`, `state`, `revisions`, `comments` |
| `projection` | `api`, `wordml`, `styles`, `numbering`, `refs`, `state`, `revisions`, `index`, `threads` |
| `compare` | `api`, `package`, `xml`, `schema`, `wordml`, `styles`, `numbering`, `state`, `revisions` |
| `document` | any of the above |
| `fuzz` | `api`, `package`, `xml`, `document` |
| `bench` | `api`, `package`, `xml`, `schema`, `wordml`, `styles`, `numbering`, `state`, `revisions`, `index`, `projection`, `check`, `compare`, `document` |

**`revisions` depends on `check`** so that the Word rules have one implementation: `Compiler::finish`
runs `check::scan_unit` on each unit that holds a revision it created. `check` must never depend on
`revisions` (it depends on `threads`, `state`, `wordml` and `schema`, none of which does), so the
table stays acyclic. `check` keeps its own list of the elements the compiler renames for that reason.

**`fuzz` is the one module that is not private.** It exists only with the off-by-default
`fuzzing` feature, which makes it a public module of the crate, and a build without the
feature has exactly the public interface in `lib.rs`. The fuzz targets in `fuzz/` are separate
binaries, and `package` and `xml` are private, so the narrowest way to reach them is a module
that hands over a few functions of the form `fn(&[u8])`. Its row lists what those functions
drive, and no other row lists it. It sits below `document` so that `document`'s "any of the
above" does not include it. The dependency check rejects a row that lists `fuzz` and a `fuzz`
row above an "any of the above" row. The targets that go through `document` (`batch` and
`files`) add `document` to this row. [Decision 0002](docs/decisions/0002-fuzz-entry-points.md) records
why.

**`bench` is private and exists in test builds only** (`#[cfg(test)] mod bench;`). It registers the
benchmarks of the performance budgets, which need the internals of `package`, `xml`, `schema`, `wordml`, `styles`,
`numbering` and `state`, so its row lists what its benchmarks use and grows with them. The
rules for `fuzz` hold for it: no other row lists it and it sits below `document`. The clock, the
inputs, the budgets and the table are in `stetdoc-testkit`, because the core reads no clock.
[Decision 0003](docs/decisions/0003-benchmarks.md) records why.

## The one seam inside the core

**`projection::formats::Format`** is the only trait in the core with more than one
adapter.

**Formats render and parse the neutral model** in `projection/model.rs`, which is a
tree:

```
Projection → Story (body | header/footer by section and kind | footnotes | endnotes | comments)
Story      → Block*
Block      = Paragraph { ref, style, clause_number (read-only), inlines, flags }
           | Table { ref, rows: Row { ref, cells: Cell { spans, blocks: Block* }* }* }
           | SectionBreak { section }
Inline     = Text | Insertion { revision, inlines } | Deletion { revision, inlines }
           | MoveFrom/MoveTo { revision, inlines } | CommentStart/CommentEnd { id }
           | NoteReference { id } | FieldResult { text } (read-only)
           | Placeholder { kind, alt } | Hidden { reason, inlines } | ReadOnly { reason, inlines }
```

Formats know syntax, and nothing else in the core does.

**The adapters:**
- `json`, used for `--json` and for format-independent engine tests
- the candidate formats during the format experiments
- later, versioned formats (`stet/1`, `stet/2`) kept so old working copies still open

**Conformance.** Every adapter must pass the same suite in the testkit:
- determinism
- `parse(render(x)) == draft(x)`, where the draft is what the unedited files say (the
  neutral model cut into editable slots and read-only seams, in no syntax), so an edit is the
  difference of two drafts and the alignment is written once for every format
- **edited input:** generated edits to rendered text parse into the expected
  operations under the alignment rules
- paging and budgets

The budget is measured with a deterministic character-based estimate, never a model
tokenizer.

**No other seams.** Everything else is a concrete module. ZIP, XML, schema data and
diffing are in-process dependencies (they run in the same process and need no stand-in for tests), so they need no ports.
There is deliberately no storage abstraction:
- The working copy is a plain value (`WorkingCopy { files, complement }`).
- The CLI writes it to disk, a host application keeps the `Document` or the `WorkingCopy` between
  turns, and `stetdoc-editor` treats `export()` as its virtual files.

External tools (Word, LibreOffice) appear only in the testkit's oracle runners. Tests
that need them are skipped where the tool is absent.

## How the pieces work together

**Editing (`Document::edit`):**
1. Open a `Tx`. Each part is copied on first write, and untouched parts are shared.
2. `ops::apply` compiles the batch into the `Tx` and records the touched blocks. It
   resolves every ref against the batch's base token, and declares what a split, a merge
   or a rejected insertion does to identity. `Edit::files` first turns files into a batch
   through `projection::files`.
3. `check::edit` validates the parts the transaction changed under the no-new-violations rule.
   It finds the units that changed from the span trees and not from the operations'
   declarations, and compares each with its origin: the same element in the base, and what
   `document` declared the unit was copied from (it converts the compiler's effects).
4. Enforce the scope guard. A block counts as touched when its own bytes change in
   place: a paragraph's XML, or what a table or row owns apart from its nested blocks,
   so an edit inside a cell touches the paragraphs in it and not the table. A merge
   touches both blocks, and `add` and `reply` of a comment touch the blocks that hold their new
   markers (the cleanup of a removed comment's markers is exempt and is listed in the receipt). Commit
   itself stamps the blocks that changed without an edit of their own: moved blocks,
   blocks whose wrappers changed (wrapped into or out of a content control or a revision,
   or in a row or cell that gained or lost a revision), tables and rows that gained,
   lost or reordered rows or cells, paragraphs that start in another field state, and
   the paragraph after one whose mark revision changed.
5. Commit, or discard on an error or dry run. Commit audits the staged parts against the
   ref table (it retires the refs of removed blocks and mints refs for new ones, in
   document order), extends the lineage, bumps the state version, and keeps the derived
   caches (text model, cascade, numbering, index) that were built from parts that did not
   change. Caches are not invalidated: each records the generation of every part it was
   built from, and is rebuilt on the next read if one has changed. Discard drops the
   `Tx`, which never wrote to the committed state.

**Compare (`compare`).** Open both packages, accept any existing revisions in memory
(with a warning), then align blocks:
- clause numbers are excluded from comparison text
- unique-hash anchors come first, then similarity

It diffs the aligned blocks into revisions on a `State` built from the **revised**
package. Deleted content from the original goes through `compare::import`, which owns
cross-package transplants:
- **Relationships:** hyperlinks are re-pointed, and images are copied into new media
  parts.
- **Styles:** missing definitions are imported, and colliding ids are renamed.
- **Numbering:** definitions are imported with fresh ids.
- **Notes:** referenced notes are copied.
- **Bookmarks:** they are renumbered, and `_GoBack` is dropped.
- **Comments:** comments inside deleted content are dropped, with a warning.
- **Revision ids:** fresh ones are assigned.

`document` wraps the result into a `Document`.

**The editor (`stetdoc-editor`):**
- `view` shows a readable file with line numbers, in the mode the format fixes.
- `str_replace` requires a unique match. Otherwise it returns `ambiguous_match`, listing
  each match with context.
- `str_replace` and `insert` each commit immediately as one tracked `Edit::files` and
  return the touched blocks' new text.
- `create` returns `unsupported`.
- Paths may sit under a prefix, such as `/documents/acme/`, and a path outside it is `not_found`.

### CLI commands and the core calls behind them

| Command | Core calls |
|---|---|
| `unpack <docx>` | `open` → `export` → `write_plan(None)`, executed step by step under the lock |
| `unpack <dir> --refresh` | `resume` → `export` → `write_plan` with `refresh`; a readable file that is not UTF-8 is saved aside first |
| `view <docx\|dir>` | `open` or `resume` → `view`; a note on stderr when hand edits are pending |
| `inspect <docx\|dir> <ref>` | `open` or `resume` → `inspect` |
| `edit <dir>` | `resume` (refuses with `pending_edits` while hand edits are pending) → `edit(Edit::ops)`, with the batch naming the base token → `export` → `write_plan` |
| `edit <docx> -o <out>` | `open` → `edit(Edit::ops)`, with the batch required to name the base token → `save` |
| `edit <docx> --in-place` | as with `-o`, then an atomic write back to the input, refused with `stale_target` when the file changed since it was opened |
| `check <dir\|docx>` | `open` or `resume` → `check_pending(pending files, stamp, strict)`: the dry run of the pending files, the revisions already there by author, `check`, and the exit status |
| `pack <dir>` | `resume` → `edit(Edit::files(pending))` → `save` → `export` → `write_plan` with the pending files consumed. An existing output that is neither the original nor a .docx this working copy packed is `stale_target`, unless `--force`. `--done` removes the working copy after a pack that succeeded |
| `compare <a> <b> -o <out>` | `compare` → `save` |
| `lookup <docx\|dir> <query>` | `open` or `resume` → `lookup` |
| `capabilities` | `capabilities`, or `json_schema` with `--schema` |
| `skills get [name]` | the embedded skill (`stetdoc-cli/skill/*.md`) with tables from `capabilities` and the command tree |
| `init` | no core call: the skill stub for the agents picked or detected, and with `--project` the `AGENTS.md` block, written by `fsio` |
| `mcp` | each tool runs its command's function in process; the tools are walked from the command tree |

Every command on a directory takes an advisory lock on `.stet/lock` first, and finishes or
undoes an interrupted write (`WorkingCopy::recover`) before anything else. The author comes from
`--author`, otherwise from `STETDOC_AUTHOR`, otherwise from the name saved with `stetdoc init`,
otherwise the command fails with a usage error. The
date defaults to the CLI's clock. The flags, the
output, the exit codes and how a working copy is written are in
[`docs/design/cli-and-skill.md`](docs/design/cli-and-skill.md).

## Performance

| Operation, 300-page contract, linux-x64 | Budget |
|---|---|
| `open` (block scan; parts parsed lazily) | ≤ 300 ms |
| `view` (outline or one page of text) | ≤ 200 ms |
| `edit`, 50 operations, including validation of dirty parts (dry runs cost the same) | ≤ 1 s |
| `save`, `export` | ≤ 1 s each |
| Peak memory | ≤ 4 MiB + the file size + 8× the uncompressed size of the XML parts parsed |

The memory budget is relative to what is inflated and parsed, because XML compresses ten to
forty to one: the inflated main part alone can be eleven times the file. Measured as the peak
resident size of a process that opens the file, inflates the main part and parses it, the worst of
72 random documents with a main part of 300 KB or more uses 70% of this budget, and large
documents run at four to five times the parsed part. A tree takes 60 bytes for each node and 28 for
each attribute, which is 1.2 to 4 times the XML it came from, and a copy of a tree shares what it
does not change.

Staging is copy-on-write per part, with a list of identity declarations for the ref table.
Derived caches record the generation of every part they were built from, and are rebuilt
on the next read when one has changed, so an edit rebuilds only what it touched.

Two things are worth knowing. `open` hashes the whole package for the namespace id, about 3
ms per MiB of file on the development Mac (SHA-256 without hardware instructions): 3 to
15 ms for a realistic contract of 1 to 5 MiB, about 150 ms for a 50 MiB package with
media, and about 1.6 s at the 512 MiB input limit. The node arenas of `xml` are stored in
chunks that a clone shares, so a staged part costs the chunks that its edits write and not
a second copy of the arenas: no operation of the state is over 80% of the memory budget on
the inputs of the gate.

A module that blows its budget is redesigned behind its interface, not worked around by its
callers.

`cargo xtask bench` times the operations in a release build and measures their peak memory with a
counting allocator, against the budgets above and against a baseline of the platform it runs on. It
measures the container, the span tree, schema validation, the state, `wordml`, and the cascade and
numbers on a 20,000-paragraph body (the gate of 25 ms is in the benchmark). It runs nightly with the budgets as written and in CI with generous margins, and a budget, a memory
regression or a closed known gap fails it. A time regression against the baseline is a warning
until there is data on how much a runner varies. Every input is held to the budgets, the denser and
larger documents that `state` and `wordml` were measured on too, except for the named pairs of a
benchmark and an input that are known to be over the memory budget (`MEMORY_GAPS` in the testkit: compare
on the dense contract today), which end by themselves when the pair fits. Compare holds two documents, so
its rows have the budget of two inputs (`Limit::TwoInputs`). CONTRIBUTING says how to run it and read the table.

## Errors

`api::error::ErrorCode` holds every code, its documentation, its exit class and its
`next` hint. Keeping exit classes there, though they matter only to the CLI, keeps one
table. `ExitClass` is public so the CLI maps exit statuses from that table.
`cargo xtask check` fails if this table and the enum disagree, and
`cargo xtask docs` generates the [error reference](docs/reference/errors.md) from the
enum.

| Exit class | Codes |
|---|---|
| 1 operation failed | `unsupported`, `cross_block`, `readonly_region`, `scope_violation`, `io_error` (the CLI's file system) |
| 2 usage | `invalid_payload`, CLI argument errors |
| 3 target | `not_found`, `ambiguous_match`, `count_mismatch` |
| 4 stale | `stale_ref`, `stale_files`, `pending_edits`, `stale_target` (a file the CLI would write holds a version it has not seen: a pack's output, or a .docx edited in place) |
| 5 validation | `validation_failed` (the edit would add violations) |
| 6 input | `malformed_input`, `limit_exceeded`, `corrupt_complement`, `unsupported_version` |
| 7 internal | `internal` (a bug in stetdoc: report it, do not retry with another edit) |

**Policy.** From 1.0, codes and their exit classes are frozen. New codes may be added.
Existing codes are never repurposed or removed outside a major version.

## Cross-cutting rules

1. **Byte identity.**
   - Only `xml` serializes, and only `package` writes the ZIP.
   - Clean nodes and untouched entries are copied, never re-serialized.
   - Laws L1 to L6 are the acceptance tests.
2. **One path for edits.** Operations, `Edit::files`, the editor and compare all end
   up as `revisions::Compiler` calls on a `Tx`. There is never a second route. The document's stories change
   only through the compiler. The comment parts, `people.xml` and the style sheet are written by
   `comments`, because they hold no revisions, through `revisions::handle::PartWriter`, which writes and adds only
   those parts.
3. **Schema order.** Anything that inserts a child element into a WordprocessingML part
   calls `schema::insert_in_order`. Hand-ordered insertion is a bug. OPC parts
   (`[Content_Types].xml` and the relationships parts) have no element-order constraints, so
   `package` appends to them directly.
4. **Generated code** lives under `generated/` directories, with a header naming its
   generator. Never edit it by hand. Run `cargo xtask codegen`, and CI fails if it is
   stale. Generated code passes the same lints as hand-written code. If it can't, the
   only exception is an `#[expect(…, reason = "generated")]` on the generated module's
   `mod` declaration.
5. **Tests sit at the interface.**
   - Laws, property tests and snapshots go through `Document`.
   - Batch generators use only `view` output.
   - Every `Op` variant automatically runs the operation conformance suite: atomic
     failure, dry-run purity and the scope guard.
   - Variants whose spec is marked `reversible` (tracked text, formatting and structure
     operations) also run reject-all reversibility. `accept`, `reject`, comment
     operations and untracked edits are checked against their own laws instead.
   - Direct unit tests are reserved for modules with their own invariants: `xml`,
     `package`, `numbering`, `schema`, `wordml`'s story scan and text model, `refs`, `state`
     and `revisions`.
   - The laws run through `Document` in `stetdoc-testkit` (its `laws` module). The L1 and
     L2 corpus tests of `package` and `xml` stay inside `stetdoc-core`, over the fetched
     corpus. They may dev-depend on `stetdoc-testkit` for the corpus and the fixtures. That
     compiles the core twice, so the testkit API they use (`corpus`, `expect`, `fixtures`) is
     plain data and never a core type.
6. **No `unsafe` outside `stetdoc-napi`. No panics on input.** This is about our own code. A
   dependency may use `unsafe` inside if it is widely used, maintained and fuzzed, as `memchr`
   is. Each new dependency is justified in the change that adds it, and passes `cargo deny`.
   Library code denies `unwrap`, `expect` and `panic!`. The core also denies indexing and
   slicing that can panic, `unreachable!`, `todo!`, `unimplemented!`, `dbg!`, printing and
   `#[allow]`. The only way around a lint is a local `#[expect(clippy::name, reason = "...")]`
   with a real reason. Tests may panic, and so may `stetdoc-testkit`, which is test support.
   **No file I/O outside `stetdoc-cli`'s `fsio`. No clock reads outside the CLI and the napi
   layer.** The check rejects `std::fs`, `net`, `env`, `process` and `os`, and everything in
   `std::time` except `Duration`, anywhere in the core, including tests, unless a test is
   listed with a reason in `xtask/core-io-exceptions.txt`. The standard streams and aliases
   or globs of `std`, `std::time` and `std::io` are rejected too. The check works on paths,
   so methods such as `Path::exists`, `read_dir` and `metadata`, and `io::stdin()` after a
   plain `use std::io;`, are for the reviewer to catch.
7. **Size.** A file that grows past 600 lines is split along a concept. The check
   lists exceptions, each with a reason, in `xtask/file-size-exceptions.txt`.

## Where do I change X?

| Task | Files | Concepts to know |
|---|---|---|
| Add a hidden-text rule | `styles/hidden.rs`, a fixture, a snapshot | Effective formatting. Formats render `Hidden` generically |
| Add an input limit | `api/limits.rs` (field, default, docs), the place that enforces it (`package/layout.rs`, `package/mod.rs` or `xml/scan.rs`), a hostile test that trips it | Limits bound what hostile input can make us allocate or loop over, and a violation is `limit_exceeded`. The default comes from the corpus census, so say where the number comes from |
| Add an error code | `api/error.rs` (the `error_codes!` entry: variant, wire name, exit class, docs, hint), the Errors table in this file, the place that raises it | `ErrorCode` and its policy. Run `cargo xtask docs` to regenerate the reference; the check fails until the table and the reference agree |
| Add a flag to `view` | `api/view.rs` (`ViewSpec`), `projection/view.rs`, `stetdoc-cli/src/cli.rs` (`ViewArgs`) and `commands/view.rs` | The view pipeline. If the flag changes *what* is shown, the neutral model and every format change too |
| Fix a numbering bug | `numbering/counters.rs` (counter rules) or `numbering/format.rs` (a number format), a fixture, a snapshot | Numbering definitions, restarts, overrides, style links |
| Add an operation | `api/op.rs` (the payload and the `Op` variant), `api/op_spec.rs` (its entry in the table, including `trackable` and `reversible`), `ops/<name>.rs`, the exhaustive match in `ops/mod.rs` (the compiler points to it); `revisions/` only for a new revision shape | `Tx`, compiler primitives, schema order. Parsers, capabilities and conformance tests follow from the spec at compile time. Run `cargo xtask docs` to refresh the reference |
| Extend compare to a new property | `compare/diff.rs` with the matching `revisions` writer; `compare/import.rs` if the property references other parts; a compare pair in the corpus | Alignment, import, and the compare law (accept-all gives revised, reject-all gives original) |
| Add a projection format | `projection/formats/<name>.rs`, registered in `formats/mod.rs` | The neutral model. The conformance suite tells you when you're done |
| Add a CLI command | `stetdoc-cli/src/commands/<name>.rs`, the `Command` enum in `cli.rs`, `stetdoc-cli/doc/<name>.md` (its examples), the table above; then `cargo xtask docs` | `ErrorCode` exit classes. The CLI reference and the skill's command list regenerate, and a test fails while a command has no example |
| Change the agent skill | `stetdoc-cli/skill/core.md` (the workflow and rules), `operations.md`, `errors.md` or `stub.md`; the tables come from `capabilities()`. After a change to `stub.md`, `cargo xtask docs` | Which way of editing the skill works best for agents is still being measured. The stub in `skills/stetdoc/` is what `stetdoc init` installs |
| Add a fuzz target | An entry point `fn(&[u8])` in `stetdoc-core/src/fuzz/` and a row for what it drives in the Allowed dependencies table; `fuzz/fuzz_targets/<name>.rs`, a `[[bin]]` in `fuzz/Cargo.toml` and `fuzz/dictionaries/<name>.dict`; the target in `xtask/src/fuzz.rs` and in the matrix in `.github/workflows/ci.yml` | The laws of the module it drives. A panic, a hang or a broken law is a failure, and refusing hostile input is not. Run it with `cargo xtask fuzz <name>` |
| Add a benchmark | A file in `stetdoc-core/src/bench/` with a `Bench` and the function that prepares a run, a line in `all()` in `bench/mod.rs`, and the modules it uses in the `bench` row of the Allowed dependencies table; then `cargo xtask bench --update-baseline` and commit the new rows of `stetdoc-testkit/bench/baseline.tsv` | The performance budgets (Performance, above). The harness times, measures memory and judges, and the benchmark only prepares a run |
| Add a test document | `corpus/manifest/<source>.tsv` and `corpus/sources.tsv` with its pinned commit, hash, licence and expected outcome (a refusal also gets a stage, a part at `read` or `xml`, and a reason from `corpus/reasons.tsv`), and `corpus/SOURCES.md`; or build one with the testkit fixture builder (`stetdoc_testkit::fixtures`) | The sources and their licences in `corpus/SOURCES.md` |
| Update schema data | `xtask/codegen/input/` and `pin.json` (copy the new upstream files, recompute the pin), then `cargo xtask codegen` and review the diff of the tables | Generated code is never edited by hand. `pin.json` records the upstream source and the hashes of the input |
| Add a method to the TypeScript library | `stetdoc-napi/src/native.rs` (the native method, JSON in and out), `npm/index.js` (the wrapper), `npm/index.d.ts` (its signature), `npm/test/smoke.mts` and `npm/README.md` | One JSON boundary: data types cross as the core's serde, and the generated `types.d.ts` describes them. A public data type that changes needs `cargo xtask codegen`, and a test fails until it is run |
| Add a release target | `TARGETS` in `xtask/src/release.rs`, `npm/targets/<target>/package.json`, the optional dependency in `npm/package.json`, the loader's target names in `npm/index.js`, and the matrices of `.github/workflows/release.yml` | `cargo xtask release check` fails until all of them agree. Nothing is published from CI |
| Correct the schema data | a rule in `xtask/src/codegen/supplement/rules.rs` with its reason and reference, then `cargo xtask codegen` | Every correction says what the SDK data gets wrong, where the specification says otherwise, and which documents need it |

## Verifying a change

- **`cargo xtask check --quick`** is the inner loop: format, the static checks (dependency
  table, contracts, file size, docs links, error table and generated docs), then clippy
  and tests for the crates you changed relative to `main`, including uncommitted
  changes, and the crates that depend on them. A change under `corpus/` selects the
  crates that read it.
- **`cargo xtask check`** runs what the check job of CI runs:
  - format
  - clippy on all targets with warnings as errors
  - rustdoc with warnings as errors, so broken intra-doc links fail
  - all tests, snapshots and laws on the curated corpus
  - the random edit batches of the edit laws on the curated corpus, in a release build
  - the dependency-table check and the module-contract check
  - the file-size check
  - the docs link check (relative links and anchors in every tracked `.md`)
  - docs drift (skill, `--help`, CLI reference, error table and reference)
  - `cargo deny`: licences, advisories, bans and sources

  If it passes locally, the check job of CI passes. CI has two more jobs that `check` does not
  run: `cargo xtask fuzz <target> --time 60` for each fuzz target, which needs the pinned
  nightly and cargo-fuzz, and `cargo xtask bench --ci`, the benchmarks in a release build (a
  nightly workflow runs them with the budgets as written). Every check names the file and line
  and says how to fix the problem.
- **Snapshots** change only when the change intends to change output. Review them
  with `cargo insta test -p <crate> --review` and explain them in the pull request.
  `cargo xtask check` never writes snapshots. A failing law is
  never fixed by accepting a snapshot.
- **Word checks** have no desktop Word ([decision 0005](docs/decisions/0005-word-oracle.md)). Before each
  release, the hand-check pack (`cargo test -p stetdoc-testkit --test word_pack -- --ignored`) is opened in Word
  for the web, and `cargo xtask oracle libreoffice --sample 500` reads 500 outputs in LibreOffice, which tests
  skip where it is not installed.