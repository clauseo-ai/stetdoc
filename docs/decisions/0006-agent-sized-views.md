# 0006. Views sized for agents: a smaller default page, find within a range and in the ledger

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

Claude Code was run against stetdoc on real contracts. Three things cost it turns. A `view --find` on a 75,000-character
contract printed 32 KB, and a range view about as much: Claude Code spills a tool result over about 30,000 characters to a
file, and agents often chain two or three views in one shell call. The ledger of `--revisions` cuts each change to 80
characters, so an agent could not tell which changes touch an amount or a date and built its own diff. And text such as
`[DELETED: ...]` that a document holds as plain text looked like a change, so the agent unzipped the package to read the
XML. Each fix changes a type that `lib.rs` re-exports, so this records them.

## Decision

- **`Budget::default()` is 3,000 tokens**, about 12 KB of text, and `Budget::DEFAULT` names it. The CLI's `--budget`
  and the MCP view tool take their default from it, so the two agree. `--all` is a CLI flag for `--budget 0`.
- **`Scope::Find` has `from` and `to`**, both optional and `#[serde(default)]`: with `from`, a find looks only in that
  range, as `Scope::Range` reads it. A find pages like a range, and the position's scope counts the matches and the
  blocks that hold them.
- **`RevisionFilter` has `find` and `full`**, both `#[serde(default)]`. `find` keeps the changes whose inserted or
  deleted text holds it, smart case as a `Find` is, over all the text the change carries (1,000 characters each way).
  `full` makes each `RevisionInfo` carry that text and not 200 characters, and a ledger page show it whole and not cut
  to 80. `Document::revisions` honours both, so a filter means the same in a view and in the method.
- **`WarningCode::OutlineShown`** (`outline_shown`): a view of the whole body that does not fit a page shows the outline,
  and the warning says how big the body is and names the range of the whole body and `--all`.
- **`Document::inspect` of a block** lists the block's tracked changes and comments, by id, kind and author, or says it
  has none. The `Inspection` type does not change: these are more of its lines.

The `stet/1` adapter is unchanged for working copies. Only lines that a view prints change: the ledger line with
`full`, and the scope a position line carries. `json/1` writes `full` on a ledger only when it is set.

## Consequences

- A caller that built a `Scope::Find` or a `RevisionFilter` with a struct literal adds the new fields, or
  `..RevisionFilter::default()`. JSON callers (napi, MCP) need nothing: the fields default.
- A default view is a third of its old size, so a long document takes more pages. Each page says how to get the next,
  and `--all` or `budget: 0` gives everything at once.
- The MCP view tool's schema says in each property's description which arguments it cannot be used with. It does not
  use `oneOf`, which some clients refuse at the top of a tool's schema.
