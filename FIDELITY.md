# Fidelity

This file says, feature by feature, how faithfully stetdoc 1.0 keeps, shows and writes a Word document. It uses four words:

- **Exact.** Kept byte for byte, or written the way Word writes it, with nothing changed.
- **Normalized.** Kept or written in an equivalent form that Word reads the same way. Each case below says what changes.
- **Read-only.** Shown to the agent, but an edit to it is refused (`readonly_region`), never silently dropped.
- **Omitted.** Not shown, not compared or not supported in this release. The package still keeps it exactly.

Two rules hold everywhere. An unedited round trip returns the input bytes, and an edited one changes only the elements
that the edit touched (law L1 and L2). Anything stetdoc does not model is preserved exactly, because it is never rewritten.

## By feature

| Feature | Round trip | View | Edit | Compare |
|---|---|---|---|---|
| Everything in the package, including parts stetdoc does not model | exact | omitted when not modelled | omitted | omitted |
| Paragraph text, runs, hyperlinks, unlocked content controls | exact | exact text | tracked, a cleaned word-level diff (normalized) | word-level |
| Paragraph insert, delete, split, merge | exact | exact | tracked, paragraph-mark revisions | yes |
| Tables, merged and nested | exact | exact | cell text, rows insert and delete | rows and cell text; table, row and cell properties omitted |
| Table columns | exact | exact | omitted (next release) | omitted |
| Lists and clause numbers | exact | computed numbers, read-only | level, restart, removal | numbers are not compared, so renumbering is no difference |
| Number formats | exact | normalized: script formats and values out of range show decimal, with a `Note` | | |
| Styles and effective formatting | exact | exact, with the source of each value | paragraph style, run and paragraph properties, any schema property through `set`, tracked | run and paragraph formatting; style and numbering definitions omitted |
| Theme colours with a tint or shade | exact | normalized: within 2 of Word's value in a channel | | |
| Section and page setup | exact | exact | properties through `set`, tracked; adding a section break is untracked only | section properties omitted |
| Headers and footers | exact | by section and type | as paragraphs | yes; a header or footer on one side only is skipped, with a warning |
| Footnotes and endnotes | exact | exact | text; `delete-note` | yes, they follow their reference |
| Comments and threads | exact | exact, read-only in `comments.md` | `comment`, `reply`, `resolve`, `edit-comment`, `delete-comment`, never tracked | the original's comments are omitted; the revised document keeps its own |
| Existing tracked changes | exact | exact, moves shown as moves | another author's change is read-only; `accept`, `reject` | accepted first, with a warning |
| Moves | exact | shown as moves | written as a deletion plus an insertion (normalized) | written as a deletion plus an insertion, and listed as moves in the receipt |
| Fields | exact | cached results, read-only | the result is read-only; codes omitted | codes omitted |
| Equations | exact | a placeholder, read-only | a whole equation is one character; see below | excluded in deleted content |
| Images, charts, SmartArt | exact | alt text | omitted (inserting images is the next release) | charts and SmartArt in deleted content are excluded |
| Text boxes | exact | shown | | |
| Bookmarks | exact | | | |
| Hidden text | exact | flagged, with a warning | | |
| Dates of revisions and comments | | | normalized: see below | normalized |
| Pagination, page numbers, layout | omitted, never | | | |
| Macros | kept, never executed | | | |

An empty cell means the feature takes no part in that column.

## Settled details

Each of these was decided in a design, with the evidence there.

- **Dates are the caller's wall clock labelled `Z`, with no `dateUtc`.** A revision's or a comment's `w:date` is the local
  time that the caller passes (the CLI's clock in the zone `TZ` names, or the user's zone in a host application), written as `YYYY-MM-DDTHH:MM:SSZ`, which is Word's own
  convention. No `w16cex:dateUtc` is written. See [revisions 5.3](docs/design/revisions.md#53-dates) and
  [comments and notes](docs/design/comments-and-notes.md).
- **A tracked replacement is a cleaned word-level diff, not the character-minimal one.** Dense edits become one
  replacement, deletion before insertion, as a careful reviewer would write it. See
  [revisions 2.5](docs/design/revisions.md#25-cleaning-the-word-diff).
- **Moves are written as a deletion plus an insertion**, in edits and in compare. Writing true moves is the next release.
- **A deleted paragraph that carries a section break keeps its mark and the break**, so accepting leaves an empty paragraph
  that still ends the section. Word drops the break with the mark. See
  [revisions 3.2](docs/design/revisions.md#32-paragraph-marks).
- **A section break added with `set` is untracked only**, because a `w:pPrChange` cannot hold it. A tracked attempt is
  `invalid_payload`. See [document and ops 2.1](docs/design/document-and-ops.md#21-the-set).
- **A tracked deletion of an equation that already holds a tracked deletion is refused**, because Word writes a form for it
  that the schema does not allow, and stetdoc never writes a nested deletion. See
  [revisions 1.3](docs/design/revisions.md#13-word-writes-x-for-y).
- **A merge into a paragraph that begins a field spanning paragraphs is refused** (`readonly_region`): the paragraphs a table of
  contents or another such field spans are one read-only block, and a merge would move the other paragraph's text into
  it. A merge out of such a field was already refused. See [revisions 3.2](docs/design/revisions.md#32-paragraph-marks).
- **Number formats that cannot show a value fall back to decimal, with a `Note`**: a Roman numeral above 3,999, and the
  script formats (CJK, Hindi, Thai, Hebrew, Arabic, Cyrillic, `bahtText`, `dollarText`) until they are built. See
  [styles and numbering 5](docs/design/styles-and-numbering.md#5-number-formats).
- **Tinted and shaded theme colours are approximated within 2** in any channel of Word's value (89% exact on the curated
  Word documents). See [styles and numbering 3.5](docs/design/styles-and-numbering.md#35-theme-fonts-and-colours).
- **Pattern shading is not read for hidden text.** Only `w:fill` is read, so white text on a shading pattern whose colour is
  the pattern's own counts as on no background and is flagged, although it is readable. See
  [styles and numbering 3.6](docs/design/styles-and-numbering.md#36-hidden-text).
- **Compare does not compare** table, row, cell and section properties, style and numbering definitions, field codes, or the
  original's comments. See [compare 8.3](docs/design/compare.md#83-not-compared-said-once).
- **Compare excludes** equations, charts and SmartArt in deleted content, because their changed form cannot be written the way
  Word writes it. See [compare 19](docs/design/compare.md#19-the-build-and-the-quality-check-as-built).
- **A header or footer present on one side only is skipped by compare**, with a `compare_story_unmatched` warning in the
  receipt. See [compare 4.7](docs/design/compare.md#47-stories).
- **White space at the ends of a `w:t` without `xml:space="preserve"` is text**, as Word shows it (checked in Word for the
  web, hand-check file 37): the views and the offsets count it, and what stetdoc writes or copies with such white space says
  `preserve` unless an ancestor does. **LibreOffice drops it**, so a document that relies on it reads one way in Word and
  stetdoc and another in LibreOffice; `check` flags such text (`edge_space_without_preserve`). See
  [wordml 4.7](docs/design/wordml.md#47-reading-decisions-that-could-have-gone-another-way).
- **A table placed directly inside a paragraph** (`w:p/w:tbl`, which the schema does not allow) **is left out of the views,
  and edits cannot reach it.** Edits of the paragraph around it and of the blocks beside it keep it byte for byte.
- **The text of a vertically merged continuation cell is flagged hidden.** A cell with `w:vMerge` and no `restart` joins the
  merged cell that the cell above starts (ECMA-376 Part 1, 17.4.85, and `ST_Merge`: "all following cells which have a value
  of continue shall be merged into this merged cell group"), and the group is drawn as one cell with the first cell's
  content, so Word does not show the continuation cell's own text. The view keeps it, flagged hidden, so that an agent sees
  what the file holds.
- **A text edit never deletes a note's own number** (`w:footnoteRef`, `w:endnoteRef`): deleting a note's whole text keeps
  the number, as Word does. A note goes only when its reference in the body goes: the `Final` view of the notes leaves out
  a note whose reference is a pending deletion, and the `Original` view one whose reference is a pending insertion. A note's
  id (`fn3`), which refs and `delete-note` use, counts every reference and is the same in every view; the two views add the
  number Word shows as its mark once their changes are resolved, when that is another (`{footnote 3 mark=2}`, `"mark"`).
- **Quotes typed into a paragraph follow its convention**, curly or straight, and the receipt says so. `--quotes as-typed`
  keeps them as typed. See [document and ops 2.8](docs/design/document-and-ops.md#28-quotes).

## Never

Pagination, page numbers and layout; the legacy binary `.doc` format; executing macros.
