# 0021. Compare does not refuse over a problem an input had, or over a change at the edge of a field or a read-only control

- **Date:** 2026-11-17
- **Status:** accepted on 2026-11-17. Word for Mac has not run the outputs yet (folders `a1-*` of batch 5).

## Context

Clauseo found two ways a comparison went wrong over one block.

1. **A nested table that ended its cell, deleted.** Some generators (python-docx, docx.js) write a nested table as the last
   block of a cell, with no paragraph after it. Word does not allow that, and `check` reports it as
   `cell_without_final_paragraph`. When the revised document drops the nested table, the redline is built on the revised
   document and carries the table back as deleted content, so the revised cell ends with a table again. `check` blamed the
   comparison: `stetdoc compare nl_a.docx nl_none.docx` failed with `validation_failed` ("The edit would add 1 violation"),
   exit 5, and wrote nothing. The problem was the original's, but its key belongs to the revised document's cell, so the
   foreign origins that let a copy keep its problems (design `compare.md` 7.7) could not carry it.
2. **A change at the edge of a field or a read-only control.** The redline checked deleted text that goes into a gap, and
   a cut between merged paragraphs, against both neighbours of the gap. A gap right before or after a simple field or a
   locked content control has one neighbour inside it, so the redline refused, and compare fell back to replacing the
   whole paragraph (`compare_paragraph_replaced`) for a word deleted next to the field. A merge whose cut lay right after
   a field's end was refused as "inside a field", because the check counted the field's end as still to come. Since #24 and #26,
   `edit` inserts at the edge of a read-only control and a simple field takes the edits a complex field takes,
   so compare reached less far than edit.

## Options considered

For the nested table:

| Option | Strength | Weakness |
|---|---|---|
| 1. End the cell with a new empty paragraph after the deleted table, untracked | The cell is valid; Word adds the same paragraph when it opens the original | Accepting and rejecting both keep one empty paragraph that neither input had |
| 2. Delete the paragraph before the table's mark and join it to a new last paragraph across the table | Accepting gives the revised cell exactly | A join does not reach across a table the step removes (design `revisions.md`, finding 28), in stetdoc or in Word |
| 3. Replace the paragraph before the table whole, with its text inserted in a new last paragraph | Accepting gives the revised cell exactly | Unchanged text shows as deleted and inserted |
| 4. Declare the revised cell's keys foreign | Nothing changes in the output | The redline keeps a cell that ends with a table, which Word repairs on open |

For the safety net, when `check` still finds a new violation in a redline:

| Option | Strength | Weakness |
|---|---|---|
| A. Carry a violation whose code either input has, anywhere | Simple | One such problem anywhere in an input hides every new one of that code; a cell the redline breaks would ship |
| A2. Carry by origin: only a violation at a block that came from the input that had it | Exact | The violation that needs carrying sits in a container of the revised document, which has no origin in the original |
| A3. Carry by count per code, never for the classes Word repairs or refuses | A new problem shows as one more of its code than the inputs have | Content both inputs share is in the redline once, so summed counts let a new problem through; a code alone lets a new value hide behind an old one |
| A4. As A3, keyed by the code with its detail (element, attribute, value) and capped at the larger input's count | A new value, or one more of an old one, is new | Refuses when the original's paragraph with a problem is deleted and the revised document adds another with the same problem |
| B. Write the output with a warning for every violation | Compare never refuses for a violation | A defect in compare would ship a file Word may refuse |

## Decision

Option 1 for the nested table, and option A4 for the safety net. A review of the first version (option A) found that
it would have written `nl_a`'s broken cell on its own, without the empty paragraph, because `nl_a` has that code. A second
review found that A3's sum let a new invalid value through when both inputs share a paragraph with one.

- **The empty paragraph.** `Compiler::place_deleted` follows a deleted table that ends a cell, a text box, a header, a
  footer, a note or a comment with a new `<w:p/>`, live and untracked (`end_after_deleted_table`). The body is left out: its
  last block is the paragraph that holds the last section, and no placed block follows it. The paragraph is a new block with
  no origin. Compare lists it with the code `compare_not_restored`, naming its ref, and says that it stays when the changes
  are accepted or rejected. The compare laws leave it out of both sides. With it, accepting gives the revised cell and an
  empty paragraph, and rejecting gives the original cell as Word reads it, with the paragraph Word adds there.
- **The safety net.** `document::compare` runs `check::compare_located` in place of `check::edit_located`. The foreign
  origins carry what they always did: a copy of the original's content keeps its own problems. When a violation is still
  new, it counts the keys of the violations in each input (each with its revisions accepted, as compare reads them) and in
  the whole redline. A key is `check`'s: the code with its detail, such as the element, the attribute and an invalid value,
  never a place. A new violation is carried, and listed with the carried problems, only when its code is not one of the
  classes below and the redline has its key no more often than the input that has it more often. Otherwise it refuses, as
  `validation_failed`, worded "The redline would add N violations that neither input has as often". A violation of the
  whole document that no part's keys hold (two parts using one id) is never carried this way.
- **Never carried by the net**, whoever had them first, since Word repairs or refuses a file with them
  (`check::edit::NEVER_CARRIED`): the structure of tables and containers (`cell_without_final_paragraph`,
  `table_without_row`, `row_without_cell`, `container_without_block`), the schema's shape of an element
  (`schema_unexpected_child`, `schema_missing_child`, `schema_out_of_order`, `schema_unexpected_text`,
  `schema_wrong_root`), the package (`mc_prefix_undeclared`, `relationship_target_missing`, `relationship_id_duplicate`,
  `part_without_content_type`) and the comments (`marker_unpaired`, `marker_without_comment`, `reference_missing`,
  `reference_duplicate`, `comment_id_duplicate`). Carrying by origin alone was not taken: the violation that needs it is
  keyed to a container of the revised document, which has no origin in the original. The schema's attribute codes stay
  carriable: real files often have their own invalid values, and compare must work on them.
- **Known limitation.** When the original's paragraph with a carriable problem is deleted and the revised document adds a
  different paragraph with the same problem, the redline holds both, one more than either input, and compare refuses. The
  refusal's next step names this case and says to compare copies of the inputs that Word has opened and saved.
- **The edge rule.** A change right before or after a simple field, a complex field or a content control that is locked,
  bound to data or showing its placeholder is written in place. Deleted text goes into the gap outside the field or the
  control, as an insertion does (`check_redline_gap`, which refuses a gap only when both neighbours are inside the same
  simple field or read-only control). A cut between merged paragraphs, and `split_block`, use `check_gap`, and a cut counts
  the field ends that close right at it as passed, as `split_block` settles it.
- **Inside a read-only region.** A change strictly inside a read-only control, or inside a simple field's result, is not
  written in place: the paragraph is replaced whole, with the `compare_paragraph_replaced` warning. That rule was there,
  and it now covers only what is truly inside. A simple field's result is not edited in place because the original's
  deleted result is copied as a complex field, which cannot stand inside a `w:fldSimple`. Text inside a complex field's
  result is still written in place, as before. Compare never refuses the whole comparison over a change it cannot write.

## Consequences

- `compare nl_a.docx nl_none.docx` writes a redline with two deleted rows, the empty paragraph and one
  `compare_not_restored` warning, and `check` finds nothing on it.
- A word deleted next to a field or a locked control is one deletion, not a replaced paragraph, and so is a merge cut at a
  field's end.
- A redline can carry a violation of a carriable code into a new place, as long as it has that key no more often than the
  input that has it more often. The receipt lists it as carried.
- Not changed, and recorded in `FIDELITY.md`: a deleted copy does not keep a content control, inline or block-level, so
  Reject All gives the text back without it. That is the import's rule (design `compare.md` 7.2) and holds on c4d3ebb too.
- Word for Mac checks the empty paragraph and the edge deletions in batch 5 (`a1-*` folders). If Word drops the empty
  paragraph on accept, or keeps it differently on reject, the warning's wording changes, not the rule.
