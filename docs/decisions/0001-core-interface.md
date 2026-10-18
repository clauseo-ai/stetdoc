# 0001. The core interface: one handle, one format seam

- **Date:** 2026-10-05
- **Status:** accepted on 2026-10-05

## Context

`stetdoc-core` sits behind the CLI, the napi library in a host application, the backend for
Claude's text editor tool, every test, the format experiments, and future bindings.

The project's benchmark for the codebase is how quickly a fresh agent or person, with no
prior context, can contribute correctly. So the interface has to do two things: give
callers as much leverage as possible, and be legible from `lib.rs` alone.

We followed the *design it twice* process from John Ousterhout's *A Philosophy of Software Design*. Four designers
worked under different constraints.

## Options considered

| Design | Shape | Strength | Weakness |
|---|---|---|---|
| 1. Minimal | `open`, `capabilities`, `Session::run(Request) -> Response`. Every capability is a serde request | Three functions. Bindings are JSON marshalling. One fuzz target. Atomicity implemented once | The concept count moves into the `Request`/`Response` types. Callers unwrap enums. Capabilities aren't visible as methods. `Compare` mutates the session |
| 2. Flexible | An `Engine` with a builder that registers operations, formats, revision kinds, stories, check rules and compare facets as trait objects | One registration produces parsers, capabilities, skill text and tests. Conformance suites per trait | A large interface (engine, builder, six traits). Stringly-typed arguments. Indirection without third-party users. Its author recommended collapsing most traits back into enums |
| 3. Common caller | One `Document` handle: `view`, `edit`, `save` as the default loop. An `Edit` builder. `Stamp` with no default. Auto view defaults. Receipts carry post-edit text | The default loop is three calls with nothing to thread through. Safe defaults. Methods are discoverable | About ten methods on the facade. A `Drift` policy for hand-edited files |
| 4. Ports and adapters | A `Document` facade, plus a `WorkingCopy` over a `StateStore` port and a `ProjectionFormat` port with a neutral render model | The neutral model keeps format syntax out of the engine. A JSON adapter allows format-independent tests | Its author found little leverage in the store port. Callers pass a format to every `view` |

## Decision

A combination, built on design 3:

- **From design 3:**
  - the single `Document` handle
  - the `view` → `edit` → `save` loop
  - the `Edit` builder, tracked by default
  - a `Stamp` that can't be defaulted
  - automatic view defaults within a budget
  - receipts that carry post-edit text
  - `save` that always validates
  - the complement as plain bytes (`export`/`resume`)
  - the editor backend as a client of the public interface
  - batch generators built only from `view` output
- **From design 4:**
  - the neutral projection model between the engine and formats
  - a JSON format adapter for `--json` and engine tests
  - the per-format conformance suite
- **From design 2:**
  - one operation spec as the single source for parsers, capabilities, the skill
    reference and conformance tests

  It is implemented as a closed `enum Op` with exhaustive matching, not as a registry
  of trait objects.
- **From design 1:**
  - `serde` and `schemars` on every public data type, so bindings, `--json`, the JSON
    Schema and the CLI reference are all generated
  - a layering check in `cargo xtask check`
- **Dropped:**
  - the `StateStore` port, because three of four designs removed it and the fourth
    found little leverage
  - the registry traits
  - the `Drift` enum: `WorkingCopy::pending_files()`, plus the rule that hand edits
    enter only through `Edit::files`, covers the same ground with one concept fewer
  - `Compare` mutating a session: compare is a free function

The result is in [`ARCHITECTURE.md`](../../ARCHITECTURE.md).

### Changes after independent review

A reviewer on a separate seat from the four designers found three blockers, all fixed
in `ARCHITECTURE.md`:

1. **Hidden dependency cycles.** The staging type lived above its users, `compare`
   returned a type from the module above it, hidden-text rules sat below the cascade
   they need, and styles and numbering depended on each other.
   - A low `state` module now owns `State` and the copy-on-write `Tx`.
   - `compare` returns a `State`, which `document` wraps.
   - Hidden-text rules live in `styles`.
   - Style and numbering *definitions* live in `wordml`, so `styles` and `numbering` no
     longer depend on each other.
   - An explicit allowed-dependency table replaces the ordered list, and
     `cargo xtask check` enforces it.
2. **Refs could silently resolve to the wrong block after saved bytes were
   reopened.**
   - Every open now starts a **ref namespace**, carried in every view and receipt. A
     batch from another namespace is rejected as `stale_ref`.
   - Callers keep the `Document` or `WorkingCopy` across turns.
3. **The validation rule contradicted itself.** Inputs that already fail validation
   must still round-trip byte for byte.
   - Validation now runs inside `edit`, before commit, under a **no new violations**
     rule.
   - `save` only serializes.

The review's should-fix items are also folded in:

- **Concrete fixes:**
  - copy-on-write staging, plus performance budgets per operation
  - alignment rules for hand-edited files, and their base (namespace and state
    version)
  - `stetdoc-editor` as its own crate, so the compiler enforces "public interface
    only"
  - an owner for compare's cross-package import
  - a tree-shaped neutral model, with edited-input conformance tests
  - limits on `resume` and `compare`, a versioned complement that isn't trusted
  - new error codes and a stability policy
  - deterministic identifiers without a clock
  - a reversibility flag per operation
  - a concurrency statement
  - a table mapping CLI commands to core calls
- **Renames:**
  - the reference-graph query is now `lookup`, so "refs" only means block references
  - character ranges are "text ranges"
  - "tracked mode" is now "tracked editing"

### Addendum, 2026-10-05: the error vocabulary is public

The repository scaffold added `ErrorCode`, `StetError` and `ExitClass` to what
`stetdoc-core` re-exports, ahead of `Document`, so every module can return
`StetError` from the first day. `ExitClass` is public because the CLI maps its exit
statuses from the same table as the codes. Default `next` hints are surface neutral, and
the CLI overrides them where a command is the better answer. The rest of the public
interface is unchanged.

### Addendum, 2026-10-05: an `internal` error code in exit class 7

The review of the text model added `ErrorCode::Internal` (`internal`) and `ExitClass::Internal`
(exit status 7, "internal"). It is the answer to a failed consistency check inside stetdoc, for
example a ref declaration that contradicts the trees at commit. It is a bug in stetdoc, never the
caller's mistake, so it is not an operation failure (exit class 1, which tells a caller to rephrase
and retry) and it is not `validation_failed` (which callers must be able to tell from our bugs).
Its hint says to report the bug with the input that caused it, and never suggests another edit.
Both enums are `non_exhaustive`, so this adds no breaking change, and the codes and exit classes
freeze at 1.0 as before. The same review settled that an operation batch names a base token (the
namespace, state version and lineage), which `docs/design/refs-and-state.md` specifies.

## Consequences

- **The core has exactly one internal seam: projection formats.** Everything else is a
  concrete module tested through `Document`.
- **Adding an operation touches three or four files.** The compiler's exhaustive
  matches point to each one, and the conformance suite tests the new operation
  automatically.
- **A flag that changes what views show touches the neutral model and every format.**
  That is the price of keeping syntax out of the engine.
- **One `Document` per file in a host application.** Concurrent edits to the same document must be
  serialized, because `edit` takes `&mut self`.
- **Multi-turn hosts keep the `Document` or its `WorkingCopy`,** not just the saved
  bytes. Reopening saved bytes starts a new ref namespace, and refs from before are
  rejected.
- **Revisit** if a real third-party extension need appears: design 2's registry is the
  path. Also revisit if the neutral model can't express what a winning format needs.
