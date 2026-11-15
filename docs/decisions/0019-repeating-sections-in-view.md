# 0019. The view marks the rows of a repeating section's items

- **Date:** 2026-11-15
- **Status:** accepted on 2026-11-15

## Context

Issue 27 showed a table whose first rows are the items of a repeating section: a `w:sdt` with `w15:repeatingSection` around
`w:sdt` items with `w15:repeatingSectionItem`, one row each. The view printed those rows as `r1 {row}`, the same as a plain
row, so a caller could not tell before an edit that a `row` insert next to them makes a new item and that deleting one
removes its item. The view shows inline content controls with `{ctl ..}`, and nothing about the controls around rows.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. A word in the row line, `r1 {row item=1}` | The row line already carries the row's facts (`header`, `inserted`); one token per row, no new line | Changes the row line of the `stet/1` adapter, so old copies must keep working |
| 2. Structure lines around the items, `{repeat}` and `{/repeat}` | Shows the section and the items as nesting | New structure lines are read-only lines an old copy does not have, and the alignment would refuse every old copy of such a table |
| 3. Nothing in the view, only the docs | No format change | The caller still cannot see it coming, which is the issue |

## Decision

Option 1.

- **The token.** A row that is the content of a repeating-section item shows `item=N` in its row tag, after `header` and
  before a row change: `r2 {row item=2}`, `r5 {row header item=1 inserted #4 JD}`. `N` is the item's number in its section,
  from 1, so the rows of one item share a number and a new section starts again at 1. A row that is not in an item shows
  nothing new. It is in the style of `{table cols=3}`, a named value in a structure marker. It is a structure line, so it
  takes no characters of any paragraph's text and moves no offset.
- **Display only.** The number is shown and never edited. The alignment of an edited file with its baseline compares a
  row line without its number when the edited line has none (`Key::same_structure`), so a row line written before the token
  existed matches. A row line that shows a number must show the baseline's, as every other structure line must say what the
  baseline says: a changed or added number is refused with `readonly_region`, as a changed row line is. Nothing is dropped or
  reinterpreted.
- **The model and JSON.** The neutral model's `Row` has `item`, and the `json/1` view of a row has `"item"`, the number or
  `null`. That is an added field, which 5.8 of `docs/design/projection-and-index.md` allows.
- **Additive to `stet/1`.** AGENTS.md says a released format adapter is not touched and old working copies must still open.
  5.10 of `docs/design/projection-and-index.md` says `stet/1` is a working name until agent evals settle the format, and the
  adapter has had additive changes since its first release (1eb9969, dbe6282, eed3af1). This change is additive in the same
  way: every file a 0.9.3 build wrote parses to the same draft as before, since a row line with no `item=` reads exactly as
  it did, and it packs against a baseline that now shows the numbers. Nothing that exists today changes meaning.
- **Release skew.** A working copy records the hashes of its files as rendered, and a pack refuses files that this release
  renders differently (`stale_files`), because every difference would look like an edit. A 0.9.3 copy of a table with
  item rows renders differently now, so when the hashes differ, `document` also renders the files with no item numbers,
  as 0.9.3 did, and takes a copy whose hashes match those. This shares the fallback of decision 0017: one rendering
  with every marker added since 0.9.3 turned off (`projection::hashes_as_earlier_release`) covers both the label and the
  item numbers. Only a state with such a marker pays for the second rendering, and only when the first check fails.
- **Not the other way.** A working copy that holds the new token does not pack in 0.9.3: it refuses it with
  `stale_files`, since it renders those files without the numbers, and its parser would refuse `item=` in a row line.

## Consequences

- A caller sees which rows are items before a `row` insert or delete, and the skill's core text says what those do.
- Every table row in `view --json` has one more field.
- A repeating section that holds paragraphs rather than rows, or a table, is still not shown; the view shows no block-level
  content control. The token covers what issue 27 needs: rows.
