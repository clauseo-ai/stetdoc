# 0018. A delete removes a paragraph its container does not need, and warns when it keeps one

- **Date:** 2026-11-15
- **Status:** accepted on 2026-11-15

## Context

Issue 25 showed that a `delete` of a whole paragraph emptied it and kept it in several places, and that the receipt did not
say so. It said `0 delete p2: deletion #0`, `retired` stayed empty, and the next view still listed `p2`, now empty. A caller
that keeps its own map of the document (refs, ids, an outline) took the paragraph for removed. The rule was that a paragraph
lost its mark only when a paragraph followed it in its container, so the last paragraph of every container, and every
paragraph before a table, stayed. Some of those are needed: a cell and the body must end with a paragraph, and two tables
with nothing between them become one. Others are not: a cell or a body can start with a table, and the last paragraph of a
cell or of the body can go when a paragraph is left before it. Because the last paragraph of the body always stayed, an
agent that wanted it gone deleted its text and merged it into the paragraph before, and an insertion after that paragraph
in the same batch then had no place.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. Keep the old rule and add only a warning | Nothing in the files changes | Paragraphs that Word does not need still stay, and the final-paragraph workaround stays too |
| 2. Remove every paragraph that its container does not need, and warn for the ones that stay | A delete means removal wherever that is possible, and says so where it is not | A deleted mark on the last paragraph of a container is a shape Word does not write for the body (it deletes the mark before instead), so it is a Word check |
| 3. Option 2, and a flag that refuses to delete a required paragraph | A caller that wants removal or nothing gets it | A new flag on every surface, for a case the warning already makes visible |

## Decision

Option 2.

- **What is required** (`revisions/required.rs`). A paragraph is kept when removing it would leave a cell, the body, a
  header, a footer, a note, a comment or a text box with no paragraph, or ending with a table or another block; when it
  stands between two tables; when it is the body's last paragraph and the paragraph before it ends a section, which would
  leave the last section empty; when it is the last paragraph of a block-level content control or another wrapper of
  blocks; and when it holds a section break. A container must end with a paragraph directly, so its last paragraph stays
  when only tables, block-level content controls, custom XML elements or `w:altChunk` parts follow it. The last is a choice of stetdoc and not a rule of Word, which drops the break
  with the mark: stetdoc keeps it, as `FIDELITY.md` says, so that a section does not lose its headers and footers. The
  neighbours are read as accepting would leave them, so a paragraph whose mark is already deleted does not count as the
  paragraph that is left. A paragraph with no paragraph after it whose deletion leaves other content, such as the shell of
  a content control, is kept too, since accepting would drop only the revision of its mark. So is one that holds
  bookmarks or comment ranges with no paragraph before it to take them, and one right before a `w:altChunk`, across which
  a deleted paragraph cannot be joined. So is a last paragraph that still holds another author's pending deletion, which
  accepting only the author's revisions would leave. Tracked and untracked deletions apply the same tests.
- **Every other paragraph goes.** Tracked, its mark is deleted, and accepting removes it and moves its range markers to the
  paragraph before it. Untracked, it is removed at once, the same way. The last paragraph of a container is the
  exception, tracked: Word cannot accept a deleted mark there (Word for Mac 16.114.4 left the body's revision pending and
  removed a whole table for a cell's), so its mark stays and the mark of the paragraph before it is deleted, as Word's own
  gesture does, with an alignment change that keeps that paragraph's properties on the survivor (amended 2026-11-16).
- **A kept paragraph is reported.** Its content is deleted and the empty paragraph stays, as before. The compiler's
  `Effect::kept` names it with the reason, and `ops` turns each one into a warning with the code
  `kept_required_paragraph`, the paragraph's ref in `Warning.block`, and a message that names the ref and the reason, such
  as `p2 is the only paragraph of a cell; its text was deleted and the empty paragraph kept`. The text receipt says
  `p2 emptied and kept` in the operation's line, before its revisions, and lists the warning with the others.
- **New public item.** `WarningCode::KeptRequiredParagraph`, `kept_required_paragraph` on the wire. A warning code is
  public by kind in decision 0004, so its list needs no change.

## Consequences

- A delete of the first paragraph before a table, of a paragraph between a paragraph and a table, and of the last
  paragraph of a cell or of the body after another paragraph now removes it, and accepting retires its ref. Deleting the
  last paragraph and inserting after the one before it works in one batch.
- A receipt that kept a paragraph says so in its operation line and its warnings, in the text and in the JSON. A receipt
  that kept none is as it was.
- A range delete whose last paragraph goes whole now removes that paragraph too when the container does not need it,
  instead of leaving it empty.
- Checklist C3 in the revisions design asked a person with Word to open a file with a deleted final mark of the body. Word
  kept an empty paragraph with the revision pending, and in a cell it removed the table, so the rule for a container's
  last paragraph was revisited as above.
- A removed paragraph is gone for the rest of the batch: an `insert` after it or a `merge` with it later in the batch is
  refused (`readonly_region` tracked, `stale_ref` untracked), and the message says to insert after the paragraph before
  it, or to delete only its text so that it stays. A tracked delete of a container's last paragraph keeps that
  paragraph's mark, so an insertion after it still has a place, and accepting leaves the new paragraph after the one
  before it. After accept all, its ref is retired as merged into the paragraph before it.
- Out of scope: a flag that refuses instead of keeping. A caller that wants it can treat the warning as a failure.
- A caller that matches `WarningCode` exhaustively needs a wildcard arm, as it is `non_exhaustive`.
