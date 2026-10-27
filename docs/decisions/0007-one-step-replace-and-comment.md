# 0007. One-step replace and comment, and `Document::occurrences`

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

The most common edit an agent makes is to change a few words: "net 30 to net 60, and Delaware law to New York law".
With `edit` that is a view to find the text, a view or a receipt for the refs and the base, and a batch for each change.
Claude Code took about ten steps for a redline of two clauses. Every step is a turn, and every turn between the view and
the edit is a chance for the base to go stale.

## Decision

- **`Document::occurrences(&self, needle: &str, scope: &Scope) -> Result<Occurrences, StetError>`** lists every place a
  text is, matched as an operation's `find` matches it: in a paragraph's Final text, across runs and formatting, exactly,
  or with curly and straight quotes the same when nothing in that paragraph matches exactly. Text that a tracked
  insertion added counts. Text that only a tracked deletion still holds is not a target, and is listed apart in
  `deleted`, so a caller can say why the text was not found. `Scope::Auto` is the body, the notes and the headers and
  footers; `Range`, `Clause` and `Story` select as a view does; any other scope is `invalid_payload`. Each
  `Occurrence` carries the paragraph's ref, its story, the clause it is in, its `occurrence`, its character offsets
  and a one-line snippet, and `Occurrences` carries the base it describes. Both are new public types.
- **`stetdoc replace FILE OLD NEW [OLD NEW ...]`** and **`stetdoc comment FILE ANCHOR TEXT`** open the document, call
  `occurrences`, and build one batch of `replace`, `delete` (an empty NEW) or `comment` operations on the refs and
  offsets found, against `base()` of the same opened document, then apply it with `edit` and write it as `edit` does
  (`-o`, `--in-place`, or the working copy). Nothing can change between finding and editing, so they need no base.
  The operations, the laws and the validation are those of any batch.
- A text must be found once. Zero is `not_found` and more than one is `ambiguous_match`, and either lists every place.
  `--all` takes every place; `--expect N` takes every place only when there are N, and is the new code
  **`count_mismatch`** (exit 3, target) otherwise. A refusal of one pair applies none.
- The operations on one paragraph go from its end to its start, so an earlier one never moves the offsets of a later one.
  Two pairs whose places overlap are `invalid_payload`.
- The MCP tools come from the same `clap` tree as every other tool. The replace tool takes its pairs as an array of
  `{old, new}` objects, which the server turns into the positionals.

## Consequences

- An agent changes text in one call, and the receipt carries the written file's base as `edit`'s does, so a later batch
  can chain from it.
- `Document` has fourteen methods. napi does not expose `occurrences` yet.
- The match is case-sensitive, as `find` is, unlike `view --find`, which folds case for a needle with no capital. A
  refusal says what was matched, so an agent can retry.
- A clause label in a hit is the clause in force at the paragraph (the last one at or before it), as the index reads it,
  so a recital after a numbered party block may carry that block's label.
