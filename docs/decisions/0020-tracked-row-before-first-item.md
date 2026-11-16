# 0020. A tracked row before a repeating section's first item goes above the section

- **Date:** 2026-11-16
- **Status:** accepted on 2026-11-16, to be confirmed by a Word for Mac pass

## Context

Issue 27 made a `row` insert next to a repeating-section item a new item beside that item. Word for Mac 16.114.4 checked
the result. A tracked insert after item one and after item two passed: Reject All gave back two items. A tracked insert
before item one failed: Reject All removed the section and every item, and kept the rows "Item one.", "Item two." and
"Plain row." as plain rows. It happened twice on fresh copies. Word's own re-save of the input kept the section, and so did
stetdoc's own reject.

stetdoc's XML for that case differs from the passing case only in where the new item sits. Its `w:sdtPr` is a new
`w:id` and `w15:repeatingSectionItem`, with nothing copied from the section. The most likely cause is where Word keeps a
control that wraps rows. Word stores the control's start in the first row it wraps. A new first item puts the section's
start in the tracked inserted row, Reject All deletes that row, the start goes with it, and Word unwraps the section. If
that holds, any tracked inserted row at the top of a section fails, inside a new item or inside item one alike, so no
item encoding survives Reject All there.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. A tracked row before the first item goes above the section, as a plain row, with a warning | Reject All touches no control's start; the edit is still made and tracked | After Accept All the row is not an item |
| 2. Refuse a tracked insert before the first item | Writes nothing Word mishandles | The edit cannot be made tracked at all |
| 3. Keep the first row, change its text to the new cells tracked, and add a tracked copy of item one as a second item | Accept All and Reject All both come out right | Only for one-row items with plain content; copying fields, bookmarks and comments risks duplicate ids; Word's revision list shows a delete and two inserts for one new row |

## Decision

Option 1.

- **The rule.** A tracked `row` insert with `place` `before`, anchored on any row of the section's first item, puts the
  new row before the section, and before each row-level wrapper above the section that starts with the same row, as a
  plain row (`row_controls::before_first_item`). Its cells follow the section's first row, the row it lands next to.
  Before a later item, or after any item, it is still a new item. Untracked, nothing is rejected, so it is still a new
  first item.
- **The warning.** The compiler's `Effect::outside_section` names the anchor, and `ops` turns each into a warning with
  the code `row_outside_repeating_section`, the anchor's ref in `Warning.block`, and a message that says the row is above
  the section and not an item, why, and that an untracked insert makes it the first item.
- **New public item.** `WarningCode::RowOutsideRepeatingSection`, `row_outside_repeating_section` on the wire. A warning
  code is public by kind in decision 0004, so its list needs no change.

## Consequences

- Tracked, the new row is not an item after Accept All, and a later Word user who adds items does not get copies of it.
  The receipt says so, so the caller can choose to insert untracked instead.
- The cause is a theory until Word confirms it. The batch for Word holds the tracked row inside item one as its first row
  (the theory says that fails too), the old encoding as a known failure, and this rule. If the row inside item one
  passes, the theory is wrong and this rule is revisited.
- The same cause would hit a tracked deletion of the first item's row while other items remain: accepting it deletes the
  row that holds the section's start. That case goes to Word first, and is not changed here.
- A tracked row before the first row of another row-level control, not a repeating section, may meet the same Word
  behaviour. It is not changed here.
