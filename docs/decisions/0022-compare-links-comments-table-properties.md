# 0022. Compare shows a changed hyperlink address, an edited comment and changed table properties

- **Date:** 2026-11-17
- **Status:** accepted on 2026-11-17. Word has not yet judged the table property changes or the comment revisions (word
  batch 5, folders `a2-*`).

## Context

Three differences between two documents came out of `compare` with no revision and no warning, so the redline looked
complete and was not:

1. **A hyperlink whose address changed and whose text did not.** The address is not in a paragraph's text, so the two
   paragraphs compared equal. Accepting the redline gave the revised text with the revised address, and rejecting it
   kept the revised address too.
2. **A comment whose text was edited.** The comments part was not one of the stories compare reads. The redline kept the
   revised comment, and Reject All could not give the original's back.
3. **Table, row and cell properties** (`w:tblPr`, `w:trPr`, `w:tcPr`). Design 8.3 listed them as not compared, and the
   receipt said nothing about a pair that differed only there.

## Decision

Write the tracked form OOXML has for each difference, and warn, naming the block, where it has none.

- **Hyperlinks.** Each paragraph lists the ranges of its links, `w:hyperlink` and `HYPERLINK` fields, keyed by what they
  point to: the relationship's target, `w:anchor` and `w:docLocation`, or the field's instruction. In the text two paired
  paragraphs have in common, a stretch whose link keys differ becomes a replacement, widened to the whole link on both
  sides. Each side's range comes from the text the two have in common, so a link whose text changed with its address
  (`the portal` to `the web portal`) is replaced as exactly that link on both sides. OOXML has no change element for an address, so the original's link is deleted and the revised one inserted. The
  deleted link goes next to the revised one, never inside it. Text that no link held in the original goes outside a
  revised link that starts or ends there. A word changed inside a link that kept its address is still a word change
  inside the one link. The deleted link's address gets a new external relationship when the revised part has none for it.
  Before this, the link was dropped and `compare_hyperlinks_unlinked` counted it.
- **Comments.** Two comments pair the way notes do: their `w:commentReference`s sit at the same place in the unchanged
  text of two paired body paragraphs. Several comments at one place (comments on the same text) pair by content, never
  by id, because Word renumbers comment ids on save: first the same author, initials and text, then the same author and
  initials when that is unique on both sides. The id only orders comments that are otherwise identical. One comment on
  each side pairs as it is. Reply links in `commentsExtended` are not read. Comments
  left on both sides of such a place cannot be told apart. They are not compared, and each revised one gets a
  `compare_not_tracked` notice, rather than pairing the wrong two and swapping their texts on Reject All. A comment
  whose reference moved to another paragraph is compared with nothing, and gets the same notice when the original's
  comment of the same id reads differently. A paired comment's paragraphs are compared like a note's, so an edit is a tracked
  insertion and deletion inside `comments.xml`, and Accept All and Reject All give each side's text. A comment with no
  pair is not compared, because no tracked change adds or removes a comment. A paired comment whose author or initials
  changed has no tracked form either. The redline keeps the revised comment's, and the receipt gives a new warning,
  `compare_not_tracked`, with the ref of the comment's first paragraph and both values.
- **Table properties.** A paired table compares its `w:tblPr`, and each paired row its `w:trPr`. The cells of a paired
  row compare their `w:tcPr` by position, since rows only pair when their cells have the same shape. Properties are
  compared the way paragraph formatting is: after the same ignore list (`w:cnfStyle` and the like), without `w:tblLook`
  (which parts of the table style apply, which Word rewrites freely), and with style ids as written. A snapshot still
  holds the original's `w:tblLook`. A difference becomes a `w:tblPrChange`, `w:trPrChange` or `w:tcPrChange` holding the original's properties, in
  a container that is created if the revised element has none. A change that cannot be written is listed as an exclusion
  (`unredlined`). The receipt counts these changes in `counts.props`.

The grid (`w:tblGrid`), the row's table property exceptions (`w:tblPrEx`) and the cell layout that fitting a grid
rewrites (`w:gridSpan`, `w:gridBefore`, `w:gridAfter`) are still not compared. A change in cell shape still means a row is
deleted and inserted (`compare_table_replaced`). Section properties, style and numbering definitions are not compared
either.

## Consequences

- The compare laws read more. The dump that N1 and N2 compare lists table, row and cell properties without the grid
  layout and `w:tblLook`. Each stretch of text names the address of its link, and each referenced comment's text is
  listed. N2 leaves out the comments a notice names as not compared, on both sides, and compares the rest when both
  sides have as many. A test runs every combination of a link's text growing, shrinking, staying or changing with its address kept,
  changed or taken away, and the text around it kept or edited, through the laws in both directions.
- One curated pair changed class: `dcb:interaction.word_hyperlink_field_target_retargeted` changes a simple `HYPERLINK`
  field's address. A simple field's result cannot be edited in place, so the paragraph is now deleted and inserted
  whole. It is a recorded noise outlier (`corpus/pairs.tsv`). The curated rows of the noise baseline were re-taken:
  oxpt red_chars went from 11768 to 11800 (oxpt:WC019 changes a link's address and text) and fmt_spans from 1 to 4, and dcb red_chars from 0 to 451. The full rows were
  not re-taken.
- A paragraph that splits or merges does not compare its links. That is a known limit.
- A simple `HYPERLINK` field (`w:fldSimple`) whose address changes replaces its whole paragraph, because a simple field's
  result cannot be edited in place. It is correct and coarse, and recorded as a noise outlier.
- `view` shows a table or row format change as `{rev table}` or `{rev row}`. A cell format change has no marker of its
  own, and `view --revisions` lists it. Comment revisions show in `view --story comments`.
- Word has not yet checked how the deleted link and the inserted link read side by side. Word's own compare may write a
  changed address differently.
