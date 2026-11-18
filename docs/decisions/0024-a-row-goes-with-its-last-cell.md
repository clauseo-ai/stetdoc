# 0024. A row goes with its last cell: resolving a tracked cell that is its row's only cell

- **Date:** 2026-11-17
- **Status:** accepted on 2026-11-17. Word's own Reject All is not yet recorded (word batch 5, folder
  `c-record-word-reject-lone-cellins`); the rule stands on the reasons below until it is.

## Context

Clauseo found that rejecting a tracked inserted cell (`w:tcPr/w:cellIns`) that is its row's only cell failed with
`validation_failed` ("a table row has no cell"), on 0.9.3 and on the head of pull request 22, and that a retry failed the
same way. Accepting a tracked deleted cell (`w:cellDel`) that is its row's only cell failed the same way. Both remove the
cell (design `revisions.md`, table 3B), and nothing removed the row it left with no cell. stetdoc's own check refuses a
row with no cell.

What Word does here could not be observed directly: no copy of Word was at hand. A record-only Word folder holds
Clauseo's one-cell document so that Word's own Reject All on it can be recorded.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. Remove the row with its last cell, and the table with its last row | The result is the document as it was before the insertion, or after the deletion: the cell was all the row held | A row that existed in the source goes, with its properties |
| 2. Keep the row and give it an empty cell | Keeps the row's properties | Adds an empty cell that neither version of the document had, and makes up its width |
| 3. Refuse with a clear code | Writes nothing doubtful | Leaves a pending change that can never be resolved; the issue is that it cannot |

## Decision

Option 1.

- **The rule.** When rejecting a `w:cellIns` or accepting a `w:cellDel` removes a row's last cell (directly in the row
  or in a cell-level wrapper), the row is removed as an accepted row deletion removes it
  (`resolve_table::run_cells`, then `remove_row`): range markers are hoisted, the refs of what it held go with the
  reason of the change, a vertical merge that it started moves to the row below, a row-level wrapper left with no row
  goes, and the table goes with its last row. The selector does not matter: all, by author and by id give the same.
- **Accepting a `w:cellIns` or rejecting a `w:cellDel`** removes only the marker, so the row stays, as before.
- **A row with more cells** loses only the cell, as before, and a `w:cellDel` still widens its neighbour. A row whose
  cells are all removed in one resolution goes when the last of them goes.

## Consequences

- No public item changes: the change is in what accept and reject write.
- A row's own properties (height, header row) go with it. That is what removing the row means, and the row had no
  content left to format.
- If Word's Reject All is shown to keep the row in some other shape, this record and the rule change together.
