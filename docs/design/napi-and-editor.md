# Design: `stetdoc-napi`, `stetdoc-editor`, packaging and release

This covers the TypeScript library over Node-API, the backend of Claude's text editor tool over the readable
files, the npm package with its prebuilt addons, and the release workflow that builds the CLI for seven targets. It covers
the library's two host-facing guarantees (a stale working copy never overwrites a newer head, and the clock honours `TZ`),
the supported platforms, and the library's documentation. The core is the one in [`document-and-ops.md`](document-and-ops.md), used through its public
interface only.

## 1. Decisions

- **One JSON boundary.** The addon passes every data type across as a JSON string made by the core's `serde`, and the
  package's JavaScript parses it. Bytes cross as `Buffer`. So the wire shape is exactly the CLI's `--json` and the JSON
  Schema, and the TypeScript types generated from that schema describe what JavaScript receives.
- **A thin wrapper in JavaScript.** The addon exports a low-level `NativeDocument` class and three functions. `index.js`
  builds the public surface on it: option objects, error mapping, `Uint8Array` handling. It has no npm dependencies.
- **napi-rs 3, synchronous calls only.** `#[napi]` classes and functions, no async work, no threadsafe functions, no
  finalizers beyond the class's drop. That is the subset that behaves the same under Bun and Node.
- **UTF-16 offsets by default.** The library passes `OffsetUnit::Utf16` unless the caller asks for `"scalar"`, because a
  JavaScript string counts UTF-16 code units.
- **The editor is a function over a `Document`.** `stetdoc_editor::run(&mut Document, &Command, &Stamp)` commits each
  change at once as one tracked `Edit::files`. It holds no state of its own, so a harness keeps one `Document` (or its
  `WorkingCopy`) per file between turns and nothing else.

## 2. The TypeScript surface

`index.d.ts`. Data types (`View`, `Receipt`, `Op`, `ViewSpec` and the rest) come from
`types.d.ts`, generated from `json_schema()`, and keep the core's snake_case field names. Only the request objects that
the wrapper maps onto builders use camelCase (`dryRun`).

```ts
export class Document {
  static open(bytes: Uint8Array, options?: OpenOptions): Document;
  static resume(workingCopy: WorkingCopy, options?: OpenOptions): Document;
  base(): string;
  view(spec?: Partial<ViewSpec>): View;
  edit(request: EditRequest): Receipt;
  save(): { bytes: Uint8Array; receipt: SaveReceipt };
  inspect(target: string): Inspection;               // "p42", "t3", "r17", "styles", "numbering", "sections"
  revisions(filter?: RevisionFilter): RevisionInfo[];
  occurrences(text: string, scope?: Scope): Occurrences; // where a text is, as an edit's find matches it
  lookup(query: string): Lookup;
  check(): CheckReport;
  checkPending(request?: { pending?: ReadableFiles; author?: string; date?: string; strict?: boolean }): CheckResult;
  files(): ReadableFiles;
  export(): WorkingCopy;
  textEditor(input: TextEditorInput | string, options: { author: string; date: string; pathPrefix?: string }):
    { content: string; isError: boolean; committed: boolean; receipt?: Receipt };
}
export interface EditRequest {
  base?: string; ops?: Op[]; batch?: string;          // operations: JSON ops with a base, or a batch in either syntax
  files?: ReadableFiles;                              // or hand-edited readable files, which name their own base
  author: string; date: string;
  untracked?: boolean; scope?: string[]; dryRun?: boolean;
  quotes?: "match" | "as_typed"; offsets?: "utf16" | "scalar";   // default "utf16"
}
export interface WorkingCopy { files: [string, string][]; complement: [string, Uint8Array][] }
export function compare(original: Uint8Array, revised: Uint8Array, options: CompareOptions):
  { document: Document; receipt: Receipt };
export function capabilities(): Capabilities;
export function jsonSchema(): object;
export const version: string;                        // the release, to pin against `stetdoc --version`
export const formatVersion: string;                  // the complement version a working copy is written in
export class StetdocError extends Error {
  code: ErrorCode; next: string; opIndex?: number; file?: string; line?: number;
}
```

`edit` takes exactly one of `ops`, `batch` and `files`, and `ops` needs `base`. A `WorkingCopy` is plain data that a host
can keep between turns in memory or in its own store; `resume` verifies it as the CLI's `.stet/` is verified.

## 3. Marshalling

| Direction | Form |
|---|---|
| Options, specs, filters, readable files, edit requests into the addon | one JSON string, read with `serde_json` into the core type; a bad shape is `invalid_payload` |
| Batches | `Batch::parse` on the JSON text of `{ base, ops }` or on `batch`, so the core's limits and validation apply |
| Results out of the addon | the core's `Serialize`, as a JSON string, parsed once in `index.js` |
| Package bytes | `Buffer` both ways; the wrapper accepts any `Uint8Array` |
| `WorkingCopy` | files as JSON `[name, text]` pairs; complement entries as `{ name, bytes: Buffer }` objects, because a JSON byte array would be wrong ([`document-and-ops.md`](document-and-ops.md#12-the-types-by-file) 1.2) |

`compare` returns a `NativeDocument` that holds the receipt of the comparison, and the wrapper splits it into
`{ document, receipt }`.

## 4. Errors

Every `StetError` becomes a thrown `StetdocError`. The addon throws a JavaScript `Error` whose message is `stetdoc:`
followed by the error's JSON (`code`, `message`, `next`, `op_index`, `file`, `line`), and the wrapper replaces it with a
`StetdocError` whose `message` is the one-line message and whose `opIndex`, `file` and `line` are set when the core set
them. `toJSON()` gives back the wire shape. A thrown value that does not carry the prefix is rethrown as it is: it is a
bug in the wrapper's own arguments, never a document problem.

Each native method runs its body under `catch_unwind`. The core does not panic on input, but if it ever did, the caller
gets `internal` (exit class 7) and not a crashed process.

## 5. The editor tool

Claude's text editor tool `text_editor_20250728`, tool name `str_replace_based_edit_tool`. The virtual files are the
readable files of `Document::files()`, by name (`document.md`, `notes.md` and so on), under an optional path prefix such
as `/documents/acme/`. With no prefix a leading `/` is ignored, so `/document.md` and `document.md`
are the same file; with one, the prefix itself lists the files and a path outside it is `not_found`.

| Command | Behaviour |
|---|---|
| `view` with a file | the file's text with line numbers (`cat -n` style: number right-aligned in six columns, a tab, the line), optionally `view_range: [start, end]` with `-1` for the end |
| `view` with `/`, an empty path or the prefix | the list of files, as the paths the model uses |
| `str_replace` | `old_str` must match exactly once in the file. No match is `not_found`; more than one is `ambiguous_match`, whose result lists each match's line with a line of context on each side. The edited file goes to `Edit::files` with the stamp |
| `insert` | inserts `insert_text` (or `new_str`) after line `insert_line` (0 is the top), then the same `Edit::files` |
| `create` | `unsupported`: the files are a projection of the document, and a new file has no part to compile into |
| `undo_edit` | `unsupported`, which the 20250728 tool no longer offers anyway |

A successful `str_replace` or `insert` returns the touched blocks' new text from `Receipt.blocks`, each with its line
numbers in the new file, then the receipt's warnings. A failure returns the error as text: `code: message`, the file and
line when there is one, the match listing for `ambiguous_match`, and the `next` hint. The Rust function returns
`Result<Output, Failure>`; `tool_result` renders either as the text and the `is_error` flag of a `tool_result` block. The
library's `textEditor` takes the input exactly as the model sends it (an object or its JSON) and returns `{ content,
isError, committed, receipt }`. It does not throw for a `StetError`, because the model is the one
who needs to read it.

### Threads

The addon loads in Node and Bun worker threads, and an agent harness may run every call in one. A `Document` belongs to the thread that
made it: keep one per document, in one worker, and send it work. napi-rs debug-asserts in a class finalizer that Bun's
worker termination makes fail, which aborts a development build; the workspace turns debug assertions off for the `napi`
crate in the dev profile, so development and release builds behave alike. The smoke test terminates a worker that still
holds a `Document`.

## 6. The npm package

```
crates/stetdoc-napi/
  build.rs, src/              the addon (cdylib)
  tests/types.rs              generates npm/types.d.ts from json_schema(), and fails when it is stale
  npm/                        the package `@clauseo/stetdoc`
    package.json              main index.js, types index.d.ts, exact optionalDependencies on the seven target packages
    index.js, index.d.ts      the wrapper and its types
    types.d.ts                generated by `cargo xtask codegen`
    README.md                 the library's documentation, shipped in the package
    scripts/build.mjs         cargo build of the addon, copied to stetdoc.<target>.node for local use
    scripts/pack.mjs          copies the release addons into the target packages and runs `npm pack` on all eight
    test/smoke.mts            the smoke test, run by Node and Bun as it is; test/worker.mjs is its worker thread
    targets/<target>/         `@clauseo/stetdoc-<target>`: package.json with os, cpu and libc, and stetdoc.<target>.node
```

The loader in `index.js` first tries a local `stetdoc.<target>.node` (a development build), then
`@clauseo/stetdoc-<target>`. `<target>` is `darwin-arm64`, `darwin-x64`, `linux-x64-gnu`, `linux-arm64-gnu`, `linux-x64-musl`,
`linux-arm64-musl` or `win32-x64-msvc`; glibc or musl is read from `process.report`, and both are tried when it says
nothing. A missing addon throws an error that names the target and the paths it tried. The package is ESM-importable and
`require`-able: `index.js` is CommonJS with static named exports, which Node's and Bun's ESM loaders both see.

No package has an install script, so it installs without `trustedDependencies`. The main package stays
`"private": true` in the repository, so `npm publish` refuses it by accident; publishing is a maintainer's decision, made on
the tarballs of a draft release. `cargo xtask release check`, which a test runs, fails when a package's version differs
from the workspace's, when the optional dependencies are not exactly the seven targets at that version, when a target
package's `os`, `cpu`, `libc`, `main` or `files` is wrong, when any package has an install script, or when the release
workflow does not build a target.

## 7. Release

`.github/workflows/release.yml` runs on a `v*` tag and by hand, with read-only permissions except the final release
job, and never publishes to a registry.

| Job | What it does |
|---|---|
| `cli` | builds `stetdoc` for the seven targets on a runner of each: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl` (static), `x86_64-apple-darwin` (an Intel runner), `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`. Each binary runs `--version` and `capabilities --json`, and each archive (`stetdoc-<triple>.tar.gz`, a zip on Windows) carries the binary, the licences, NOTICE and the README, with a `.sha256` beside it. The asset names carry no version, so `releases/download/v<version>/stetdoc-x86_64-unknown-linux-musl.tar.gz` is a stable URL |
| `addon` | builds the addon for the same seven targets. The musl builds run in an Alpine container (`rust:1.92.0-alpine`, through `docker run`, since JavaScript actions cannot run in Alpine on arm64), so the addon links against musl alone; they link the C runtime dynamically (`-C target-feature=-crt-static`), because a shared library cannot carry a static one, and the job fails if `readelf` finds a glibc dependency. The macOS addon's install name is `@rpath/stetdoc.node`, so no build path ships in it |
| `smoke` | runs `test/smoke.mts` on release builds: Bun 1.3.13, Bun 1.3.14 and Node 24 on linux-x64-gnu; Node 24 on linux-arm64-gnu, on both musl targets (in `node:24-alpine`, which has no glibc), on both macOS targets and on Windows; Bun 1.3.14 on darwin-arm64 |
| `package` | `scripts/pack.mjs`: the eight npm tarballs |
| `release` | on a tag, a draft GitHub release with the archives, their SHA-256 files and the tarballs |

The pull-request CI has a `napi` job that builds the addon in debug and runs the smoke test on Bun 1.3.13, Bun 1.3.14 and
Node 24 on linux-x64-gnu and on Node 24 on macOS, and type-checks the smoke test against `index.d.ts` with pinned
TypeScript, so a change that breaks the binding fails before release.

## 8. Smoke tests

`test/smoke.mts` uses only erasable TypeScript, so Node runs it with its built-in type stripping and Bun runs it
directly. It builds a small `.docx` in memory (a stored ZIP of four parts, so no fixture is committed) and checks:
open and an unedited save returning the input bytes; `view`; an edit with a UTF-16 offset after an emoji; the
revisions it wrote; `export` and `resume` keeping refs; a stale base thrown as `StetdocError` with `code` and
`opIndex`; a bad files edit with `file` and `line`; `compare` of the original and the saved package; `capabilities`
and `jsonSchema`; and the editor's `view`, `str_replace` with a unique and an ambiguous match, and `create`.
`node --test` style is avoided so that the same file runs everywhere; it exits non-zero on the first failed check.

## 9. Plan

1. The addon for `Document`, `compare`, `capabilities` and `json_schema`, the wrapper and the smoke test under Node and
   Bun.
2. The editor crate, with Rust tests over the contract fixture, and `textEditor` in the library.
3. Generated `types.d.ts` from `json_schema()` (`cargo xtask codegen`), and a type check of the smoke test.
4. The target packages, the `napi` CI job and the release workflow.
5. The library docs: the README section, a library reference page and the CHANGELOG lines.

## 10. Open

- The npm names `@clauseo/stetdoc` and `@clauseo/stetdoc-*` (the library) and `stetdoc` and `@clauseo/stetdoc-cli-*` (the CLI) are assumed and not yet reserved. The CLI takes `stetdoc`, so `npx stetdoc` works; the library takes the name `@clauseo/stetdoc` under the Clauseo organisation's scope, and stays private until the maintainers decide to publish it.
- Node LTS in October 2026 is 24 and becomes 26 at the end of the month; CI runs both until 24 leaves active LTS.
