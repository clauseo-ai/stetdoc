# 0010. Manual markup: strikethrough and underline without revisions

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

Some readers ask for a redline "with manual formatting (strikethrough and underline) rather than Track Changes": the
deleted text stays, struck through, and the new text is underlined, as plain formatting that survives any viewer and
that no one can accept or reject by accident. Harvey's indenture task asks for exactly that. stetdoc had a `format`
operation (strike, underline, colour) but no one step for it, so an agent in our benchmark wrote a Python script that made
tracked edits, accepted them and then formatted the new text, outside stetdoc. The fix is an option of the edit, which
`lib.rs` re-exports, so this records it.

## Decision

- **`Markup`** is a new enum with `Revisions` (the default), `Manual` and `ManualColor`, set with
  `Edit::markup`. A manual edit is untracked: it writes no revision.
- With manual markup a text change keeps the old text. The old text of the range and the new are diffed by words, as a
  tracked replace is, and each piece of the difference becomes the new text put in just after the old, with
  `<w:u w:val="single"/>`, and the old text with `<w:strike/>`. A deletion is struck text only, an insertion underlined
  text only, and words the two texts share are left alone. `insert` underlines its new paragraphs; `delete` of a whole
  paragraph strikes its text and keeps the paragraph, and a range across paragraphs is struck in each and never merged.
- `ManualColor` adds `w:color` red (`FF0000`) to deleted text and blue (`0000FF`) to new text, the colours of a
  law firm's blackline. Without it no colour changes, since the reader asked for strikethrough and underline and nothing
  more. The underline is single, as Word marks an insertion, in both.
- `split`, `merge`, `row` and the deletion of a table change structure that run formatting cannot show, so a manual
  edit refuses them with `invalid_payload` and says to make them in a separate tracked edit. Every other operation
  applies untracked.
- The receipt carries `markup` (left out of the JSON for `revisions`) and its text says the markup is manual, not
  Track Changes, so the agent can tell the reader. Each operation lists what it marked in `OpResult::marked` (a new
  `ManualMark`: the block, the struck text, the underlined text), and its line says so:
  `0 replace p4: struck "thirty", underlined "sixty"`, quoting at most 60 characters of each.
- The view does not mark manual formatting yet. Struck and underlined runs are ordinary run formatting, and legal
  documents underline defined terms and headings, so a marker would need a rule for which runs are markup, a new
  inline or margin node in the model with its JSON and `stet/1` rendering kept out of working copies, and a scan of
  every run's formatting on every view. `stetdoc inspect FILE pN` already shows "strike on" and "u val=single" for
  each stretch, which is how an agent checks its markup after the receipt.
- The CLI has `--manual` and `--manual-color` on `replace` and `edit` (the second alone is enough), the MCP replace
  and edit tools have `manual` and `manual_color` from the same flags, and the Node binding's `EditRequest` has
  `markup`.

## Consequences

- Manual markup is ordinary text and formatting, so the Final text holds old and new side by side ("thirtysixty"), and a
  view or receipt shows them so; `stetdoc inspect FILE pN` shows the formatting of each stretch.
- Untracked editing rules apply: text inside another author's pending change is refused (`readonly_region`), and the
  author's own pending changes are edited in place.
- A caller that built `Receipt` with a struct literal adds `markup`. JSON callers need nothing.
