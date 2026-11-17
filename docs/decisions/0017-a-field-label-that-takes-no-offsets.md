# 0017. The view marks a field label that takes no offsets

- **Date:** 2026-11-15
- **Status:** accepted on 2026-11-15

## Context

Issue 17 was fixed by giving a complex field with no `separate` no stored result and no width. A `MACROBUTTON` or
`GOTOBUTTON` written that way still shows a label in the view, taken from its field code (`FieldInfo::display_text`),
because that is what Word draws. So `{field MACROBUTTON "Click here"}` took no offsets, while the same field with a stored
result `Click here` printed the identical line and took ten. In the pull request the reporter asked for the view to say
which one it is, since an agent that counts offsets from the view cannot tell them apart.

## Options considered

| Option | Strength | Weakness |
|---|---|---|
| 1. A trailing word `label` on such a field, as `dirty` is written | Every other field prints as before, and the word says what the text is | `stet/1` gains a word, which an older release refuses |
| 2. Print such a field with an empty result, `{field MACROBUTTON ""}` | No new syntax | The agent no longer sees what Word shows |
| 3. Leave the view as it is | No change | The two fields stay indistinguishable, and an offset after one of them is off by the label's length |

## Decision

Option 1.

- **The model.** `Field::label` is true when the text is a label that a `MACROBUTTON` or `GOTOBUTTON` with no stored result
  shows from its field code. The build sets it exactly when it takes the text from the code (no `separate`, no result, no
  width), so a field with `label` always has `width` 0.
- **`stet/1`.** Such a field prints as `{field MACROBUTTON "Click here" label}`, the word after the target and before
  `dirty`. The reader accepts `label` among the trailing words. Receipts, `lookup` and the readable files all print fields
  through the same renderer, so they show the word the same way.
- **The key.** `FieldKey` carries `label`, and its equality leaves it out, as a seam's width is left out of a seam's
  equality. The document decides the flag, so it carries no edit. A working copy written by 0.9.3 has the same field
  without the word, and it opens and packs with no "changed a read-only field" refusal: its line parses to a key equal to
  the baseline's, and a copy in the old form with nothing else changed packs to no operation. A copy written now
  round-trips. Adding or removing the word by hand changes nothing either.
- **The recorded hashes.** A pack checks that the files were rendered by this release, against the hashes recorded at
  export. A copy that 0.9.3 wrote recorded its rendering without the word, and this release renders the same state with
  it, so the hashes differ. On a mismatch `document` also hashes the files as an earlier release rendered them, with every
  marker that `stet/1` gained since 0.9.3 turned off (`projection::hashes_as_earlier_release`), and takes a match. A
  rendering that neither matches is still refused as `stale_files`. The same set of markers is the place for any later
  additive marker, so there is one earlier-release rendering and not one fallback per marker.
- **`json/1`.** The field carries `label` next to `width`. A field object without it reads as `label: false`, so JSON
  written before the flag still parses.
- **Compatibility.** The change to `stet/1` is additive. `AGENTS.md` says released format adapters are not touched and
  old working copies must still open, and `docs/design/projection-and-index.md` section 5.10 calls `stet/1` a working
  name that may change until agent evals settle the format; the adapter has had additive changes since its first
  release. Every file a 0.9.3 build writes still parses and packs unchanged with this build, which a test checks with a
  file in the old form, and no token that exists today changes meaning: `label` is a new trailing word that 0.9.3 never
  writes. The other direction breaks: 0.9.3 refuses a `stet/1` file that holds `label`, with "something unexpected after
  the result of {field}", so a working copy written by this version that holds such a field does not pack in 0.9.3. A
  working copy with no such field is the same in both.
- **The law.** M1's text comparison leaves out the text of a field marked `label` instead of skipping every paragraph that
  holds a `MACROBUTTON` or `GOTOBUTTON` with no `separate`, so the rest of such a paragraph is compared too.

## Consequences

- An agent can tell a label that takes no offsets from a stored result that does, from the view alone.
- `stet/1` stays readable by this release in both forms. The format id does not change, since the old form is a subset
  of the new one, and only the direction from new to old breaks.
