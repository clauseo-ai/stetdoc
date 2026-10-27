# 0009. Where a hit is, hits in tables, changes in context, and occurrences in Node

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

Benchmark runs and the helpers that built compact find and `replace` found four smaller defects:

- A hit or an occurrence named the clause in force at its paragraph, the last clause before it. A recital after the
  parties "(1)" and "(2)" showed `clause (2)`, and the paragraph "For the avoidance of doubt, ..." after item (g) of
  clause 14.1 showed `{§14.1(g)}`. An agent that trusts the label edits or cites the wrong clause.
- A find hit in a table cell printed the cell alone (`p37 3.25%`), which says nothing without its row, and a table was
  one item, so every hit in it was listed on one page whatever the budget.
- `--context` was ignored with `--revisions`. A reviewer who wanted to read each change in its paragraph had to view a
  range for each change.
- The Node bindings did not expose `Document::occurrences`.

`Occurrence` and `RevisionFilter` are types that `lib.rs` re-exports, so this records the changes.

## Decision

- **A paragraph takes a clause label only when it is that clause or its own text.** After a list item, whose label is
  in brackets (`(g)`, `(2)`, `a)`), a paragraph with no number takes the label of the clause the list is in, and none
  when there is none. A heading that is not a clause (a paragraph with an outline level) ends the clause before it. One
  implementation, `projection::place`, serves find hits and occurrences. It reads outline levels forward once per story,
  up to the last hit.
- **`Occurrence` has `heading: Option<String>`**: when `clause` is `None`, the start of the nearest heading, cut at 50
  characters. JSON leaves it out when it is absent. The CLI's refusals print `p19 (under "RECITALS") ...`.
- **A hit line's label keeps the noun of a bare label**: `{§Schedule 1}`, not `{§1}`, which reads as clause 1. A
  dotted or bracketed label stays as it was.
- **A hit in a table shows the other paragraphs of its row**, each cut to 40 characters, at most 8:
  `p37 {§1.01} 3.25% {row: p36 "Greater than 4.25 to 1.00", p38 "2.25%"}`. `FindHit` has `row` (JSON `row`, a list of
  `{ref, text}`). A compact page is paged by hit line, not by top, and its count is of paragraphs.
- **`RevisionFilter` has `context: u8`.** 0 is the ledger as before. With 1 or more a view shows the tops that hold the
  first block of each kept change, whole as the mode shows them (the redline marks each change inline), with that many
  tops on each side, paged like a find with context (`view=changes`). `Document::revisions` ignores it. We chose this
  over refusing `--context`, because reading each change in its paragraph is the common review step, and it then takes
  one command. The CLI's `--context` no longer requires `--find`: it needs `--find` or `--revisions`, and is refused
  with a next step otherwise. The MCP view tool follows the CLI.
- **The Node bindings have `occurrences(text, scope = "auto")`**, returning `Occurrences` from `types.d.ts`. Its
  `start` and `end` count Unicode scalar values, as the core does; a caller targets a hit by `ref`, `find` and
  `occurrence`, which need no offsets, or passes `offsets: "scalar"`.

## Consequences

- A caller that built `Occurrence` or `RevisionFilter` with a struct literal adds the field or `..Default::default()`.
  JSON callers need nothing.
- `json/1` hit lines carry `row`, empty outside a table.
- A paragraph after a list now has a shallower label than before. A document whose list items carry their own
  continuation paragraphs shows the clause the list is in for them, which contains them, so the label is never wrong,
  only less precise.
