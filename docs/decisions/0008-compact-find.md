# 0008. A find lists one line for each hit and takes several texts

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

In Claude Code runs on real contracts, locating "the payment terms" took four to six views: `--find days`, then
`--find payment`, `--find 30` and `--find "Due Date"`. Each printed the matching blocks whole with a block on each side,
about 12 KB a page, and a page held only the first 9 of 38 hits. The agent needed the place, not the text, and it read
three or four pages of whole paragraphs to find it. Both fixes change `Scope::Find` and `RevisionFilter`, types that
`lib.rs` re-exports, so this records them.

## Decision

- **`Scope::Find` has `needles: Vec<String>`**, not `needle: String`. A block that holds any of them matches. JSON reads
  `needles` or `needle`, as one string or a list, so a spec written for the old field still parses. The position's scope
  counts each needle when there are several: `find "payment" (17), "days" (44): 61 matches in 48 blocks`.
- **`context: 0` is a compact page**: one hit line for each paragraph that matches, with its ref, the full label of the
  clause in force there, and about 100 characters of its text around the first match. The text is exactly the
  paragraph's, in `Final` when the mode is `Redline`, so a piece of it can be an edit's `find`. Hits sit under a line
  for each outermost clause, and a hit on a clause heading adds a tip that names the clause for a clause view and a
  lookup. With 1 or more the blocks are whole, as before. Before, 0 meant the matching blocks whole with no neighbours;
  that page is gone, and `context: 1` is the nearest.
- **The CLI's `--find` is repeatable and `--context` defaults to 0.** The MCP view tool's `find` takes one string or a
  list, and its schema says so with `anyOf`, which clients accept inside a property.
- **`RevisionFilter::find` is a `Vec<String>`**: a change is kept when its text holds any of them. JSON reads nothing,
  `null`, one string or a list.
- The compact page is a new body of the model (`Body::Hits`), which every format renders: `stet/1` as the lines above,
  `json/1` as `{"hits": {"needles", "hits", "tips"}}`. With several needles, a whole-block page's margin note names each
  needle a block holds (`{~ "payment" 2, "days" 1}`).

The snippet is about 100 characters, not 160. On the 75,000-character master supply agreement, `--find days` (38 hits)
is 7.3 KB at 160, 5.3 KB at 100 and 4.5 KB at 80. One constant, `select_find::SNIPPET`, sets it.

## Consequences

- A caller that built `Scope::Find` with a struct literal writes `needles: vec![..]`, and one that set
  `RevisionFilter::find` writes a list. JSON callers (napi, MCP) need nothing.
- A caller that relied on `context: 0` for whole blocks asks for `context: 1`.
- `stet/1` working copies are unchanged: hit lines are only in views, and like margin notes they are never in a file.
- A tool that lists ambiguous hits can call `view` with `Scope::Find` and `context: 0` and print the page, so every
  surface lists hits the same way.
