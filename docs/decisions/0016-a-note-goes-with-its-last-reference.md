# 0016. An untracked edit takes a note out with its last reference, and check reports a note nothing references

- **Date:** 2026-11-11
- **Status:** accepted on 2026-11-11

## Context

Issue 15 showed an untracked `replace` (`--no-track`) whose range covered a `w:footnoteReference`. The reference left the
body, the `w:footnote` stayed in `footnotes.xml`, and the receipt and `check` both said there was no problem. The note's text
stayed in the package, and Word showed it nowhere. Tracked, the same batch keeps the reference in its deletion, and `accept`
removes the note and says so in `Resolution::notes_removed`. FIDELITY.md says that a note goes only when its reference in the
body goes. The view already had an `unreferenced_note` warning, but it is built only for a notes story that a page selects,
and a page selects the notes that its blocks reference, so it never fired for a note that nothing references.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. Refuse an untracked range that covers a note reference | Nothing can be lost | An agent that rewrites a paragraph or a cell with a note in it, untracked, has no way to do it |
| 2. Take the note out with its reference, as `accept` does, and say so in the operation's result | The same rule as accept and an untracked `delete-note`, through the same function (`revisions::notes::remove_unreferenced_notes`) | A receipt field is added |
| 3. Leave the edit as it is and only report the orphan in `check` | No change to edits | The receipt still says nothing, and the package keeps text nobody can see |

## Decision

Option 2, and `check` reports an orphan as well.

- **The compiler marks a deleted reference.** A text deletion whose covered items hold a note reference, and the removal of a
  row that holds one, set a flag on the `Compiler`. After each operation `ops` calls
  `Compiler::remove_notes_of_dropped_references`, which, when the flag is set, removes the notes that a reference named when
  the transaction began and none names now. Tracked, the reference stays in its deletion and nothing goes, unless the
  reference was the author's own pending insertion, which a deletion removes outright.
- **New public items.** `OpResult.notes_removed`, the notes named as `Resolution::notes_removed` names them
  (`footnote (id 1)`), left out of the JSON when empty, and the finding code `unreferenced_note`. The receipt's text adds
  `and removed footnote (id 1), whose last reference went` to the line of the operation. An untracked `delete-note` reports
  the note it removed the same way. They are a receipt field and a finding code, which decision 0004 lists as public by
  kind, so its list needs no change.
- **`unreferenced_note` is a note, not a violation.** `Document::check` reports a footnote or an endnote that no reference in
  any story names. A reference inside a tracked deletion counts, since rejecting the deletion brings it back, and a separator
  (`w:type` other than `normal`) is never reported. A document that already has one can still be edited, under the
  no-new-violations rule. The edit check does not run it, since an edit takes a note out with its last reference.

## Consequences

- The reporter's batch removes the footnote, and the receipt names it. A package with an orphaned note no longer passes
  `check` with no finding.
- The cost is one walk over every story after an operation that deleted a note reference, and nothing otherwise.
  `Document::check` walks every story once more when the document has a notes part.
- The view's own `unreferenced_note` warning still fires only when a page selects the notes story.
- A caller that matches `FindingCode` exhaustively needs a wildcard arm, as it is `non_exhaustive`.
