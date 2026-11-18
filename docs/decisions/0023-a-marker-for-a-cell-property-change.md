# 0023. The view marks a cell's property change with `{rev cell #id AUTH}`

- **Date:** 2026-11-17
- **Status:** accepted on 2026-11-17

## Context

The view marks a table's property change with `{rev table #id AUTH}` and a row's with `{rev row #id AUTH}`, on the line that
the table or the row has. A cell's property change (`w:tcPrChange`, such as its shading or its width) shows nothing. Row r1
of a table that Word marked this way shows only the row's own change, `#111`, and the cell's change, `#112`, is in the
ledger (`view --revisions`) and in no line of the body view. A caller reading the view cannot tell that a cell changed, and
a reviewer who reads the redline misses it. The change is still listed and still accepted or rejected by its id.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. A `{rev cell #id AUTH}` tag on the cell's first paragraph line, in the form of `{rev row}` | The tag is the one `{rev row}` and `{rev para}` already are, with the word `cell`; the marker sits where the cell's other tags sit (`{span}`, `{vmerge}`, `{cell inserted}`) | A cell has no line of its own, so the tag is on a paragraph, and a cell that starts with a nested table has no paragraph to carry it |
| 2. A cell line of its own, `c1 {cell} {rev cell #id AUTH}` | Every cell has a place to show its change | A new line kind is a structure line that an old copy does not have, so every table with a changed cell would refuse to pack |
| 3. A marker on the row line, one per changed cell | No line of the cell's own | The row line would need to say which cell, and a column index would drift with merges |
| 4. Nothing in the view, only the ledger | No format change | The problem stays: the reader of the redline does not see the change |

## Decision

Option 1.

- **The tag.** A cell whose properties Word changed shows `{rev cell #id AUTH}` among the tags of its first paragraph, after the
  cell's other tags: `p4 {rev cell #112 B} CELL-FMT`. `cell` is the word `change_word` already gives the cell kind, so the
  tag is the generic paragraph form of `{rev KIND #id AUTH}` and the parser reads it as one. It shows the change's id and
  its author, and nothing else; the change's old and new values are not in the view.
- **Display first, edit second.** The tag is read-only, as every tag of the first paragraph is. A line that leaves the tag out
  is not a change to the cell. The alignment of an edited file with its baseline compares the tags of a line without this kind
  of tag when the edited line has none, so a working copy written before the tag existed packs. An edited line that shows the
  tag must show the baseline's, and a changed or added tag is refused with `readonly_region`, as a changed tag is.
- **Accept and reject.** The change keeps its id in the ledger, so `accept` and `reject` take `#112` as they take any id of a
  change. Nothing new is needed for them.
- **The model and JSON.** The neutral model's `Cell` has `revs`, the cell's property changes, as a row has. The `json/1` view of
  a cell has `revs` (empty when there is none), which 5.8 of `docs/design/projection-and-index.md` allows as an added field.
- **Additive to `stet/1`.** The adapter already parses `{rev KIND ..}` on a paragraph, so a line with the new tag parses
  without a change to the grammar. Every file a 0.9.3 build wrote parses to the same draft as before, since no line of such a
  file has the tag. Nothing that exists today changes meaning.
- **Release skew.** A working copy records the hashes of its files as rendered, and a pack refuses files that this release
  renders differently (`stale_files`), because the tag would look like an edit. A copy unpacked by the PR head binary has no
  tag, so when the hashes differ, `document` also renders the files as earlier releases did: with every marker added since
  0.9.3 turned off. That is the fallback of decisions 0017 and 0019 (`projection::hashes_as_earlier_release`), which now also
  turns off the cell tag. The alignment in `to_batch` then sees the same line with no tag, and the tag is not a difference.
- **Not the other way.** A working copy that holds the tag does not pack with a release that predates it: that release
  renders the files without the tag, so it refuses them with `stale_files`.

## Consequences

- A reader of the redline sees a cell's property change on its paragraph, next to the row's and the table's.
- A cell whose first block is a nested table shows no cell tag, since a table line has no tags of its own. The ledger still
  lists the change. The same limit holds for `{cell inserted}` and `{span}` today.
- The `json/1` view of every cell has one more field. Old JSON without it still reads, as the field defaults to empty.
