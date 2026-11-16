# 0020. The row that holds a repeating section's start: a tracked row before it goes above the section, and a tracked deletion of it warns

- **Date:** 2026-11-16
- **Status:** accepted on 2026-11-16. Word for Mac 16.114.4 confirmed the cause and the insert rule (batch 3).

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

Word batch 3 confirmed it. The tracked row inside item one, as its first row, lost the section and both items on Reject
All, and the rule below kept one section with two items through Accept All and Reject All. The same batch showed the
other side. Word's Accept All of a tracked deletion of item one's only row, with item two left, removed the section's
control and left two plain rows, where stetdoc's own accept keeps the section with one item.

Word's own edits, done by hand, agree. Word's tracked Table > Delete > Rows on "Item one." writes one section with two
items, and Reject All keeps them. Its Accept All also removes the section, and Word marks that in its own file:
`w:customXmlDelRangeStart` and `w:customXmlDelRangeEnd` around the starts and ends of the section and of item one,
deleted with the row. So Word's own tracked deletion loses the section on accept too. Word's Insert Rows Above on
"Item one." makes a new first item holding a copy of the row and does not track that insertion, only the text typed
into it, so Word never writes a tracked row at the top of a section.

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
- **A tracked deletion of the row that holds the start.** It is written as any row deletion: the redline is right, and
  only the control around the rows that stay is lost in Word. Refusing would block a common tracked edit. The compiler's
  `Effect::section_start_deleted` names the row when it is the first row of the section's first item
  (`row_controls::section_started_by`), and `ops` asks once the run of operations has ended whether the section still
  holds a row that no tracked change deletes (`Compiler::section_unwrapped_by_word`). If it does, the receipt warns
  `repeating_section_unwrapped_by_word` with the row's ref. The warning says that Word's Accept All removes the
  repeating-section control around the rows that remain, that the text and rows come out right, and that an untracked
  delete keeps the section. A batch that deletes every item's row is not warned, since the section goes anyway. The
  rule is kept in those two places so that an encoding copied from Word's own tracked deletion of a first item, if a hand
  check finds one that keeps the section, can replace it.
- **Items in plain controls.** An item whose row sits in a plain content control is still an item
  (`wordml::repeating_item` looks past controls that are not items), so both rules, and `item=N` in the view, see it.
  Before, such a row was not an item, and a tracked insert before it became the section's first row, the encoding that
  fails.
- **New public items.** `WarningCode::RowOutsideRepeatingSection`, `row_outside_repeating_section` on the wire, and
  `WarningCode::RepeatingSectionUnwrappedByWord`, `repeating_section_unwrapped_by_word`. A warning code is public by
  kind in decision 0004, so its list needs no change.

## Consequences

- Tracked, the new row is not an item after Accept All, and a later Word user who adds items does not get copies of it.
  The receipt says so, so the caller can choose to insert untracked instead.
- A tracked deletion of the first item's first row still loses the section's control in Word on accept. The receipt says
  so, and the caller can delete untracked instead.
- A new item copied next to an item whose row sits in a plain control holds the row only, not a copy of the plain control.
- A tracked row before the first row of another row-level control, not a repeating section, may meet the same Word
  behaviour. It is not changed here.
