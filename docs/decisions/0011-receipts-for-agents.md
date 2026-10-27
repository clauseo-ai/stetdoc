# 0011. Receipts name others' changes, an edit of a .docx writes beside it, and a find may ignore case

- **Date:** 2026-10-27
- **Status:** accepted on 2026-10-27

## Context

In Claude Code runs with Claude Haiku, an agent asked to edit a document that a colleague had already marked up never
told the user about the colleague's tracked changes, in three tries out of three: nothing `replace` printed mentioned them.
`replace` and `comment` on a .docx refused to run without `-o` or `--in-place`, and the agent wrote its output into a
temporary folder the user never saw. `replace` matched case exactly where `view --find` is smart-case, so a text the agent
had just found was refused as not there. And `stetdoc check` counted an empty `w:rPr` as a violation, which the schema
allows, so valid outputs of other tools looked invalid in our benchmark. Two of the fixes reach the core's public
interface, so this records them.

## Decision

- **`Document::occurrences_ignoring_case(needle, scope)`** finds as `occurrences` does, with letters matched whatever their
  case. Case is folded one character for one (a letter whose lower case is longer stays as it is), so every hit's offsets
  are those of the Final text and a batch targets it as it targets an exact hit. A new method keeps `occurrences` as it
  was. The CLI's `--ignore-case` on `replace` and `comment` (the MCP tools' `ignore_case`) uses it, and a `not_found`
  refusal without it lists the places in another case and says to pass `--ignore-case`.
- **`FindingCode::EmptyProperties` is a note.** It is no longer counted in `CheckReport::violations`, fails no
  `--strict` check, is not carried into a receipt as a problem, and an edit that adds one is not refused; it is reported
  as a new note.
- **The CLI names others' changes.** Every receipt of `edit`, `replace`, `comment` and `compare`, and every view but
  the ledger, carries one or two lines about the tracked changes that authors other than the current one made: how many,
  by whom, the first three with their ledger number, kind, ref and text, and "Tell the user about them." JSON carries
  them in `others`. There is no line when there are none.
- **An edit of a .docx with neither `-o` nor `--in-place` writes `NAME.edited.docx` beside the input**, the name `pack`
  gives its output. When that file exists it writes `NAME.edited-2.docx`, then `-3`, and never overwrites. An input that
  already has such a name takes the next number, so a chain of edits reads in order. The receipt says the input is
  unchanged, which file to edit next, and, when it skipped a name, that the new file lacks the edits of the one it
  skipped.
- `comment` takes `--all` and `--expect N` as `replace` does: one comment on each place.

## Consequences

- A caller that read `violations` to judge a document sees fewer, and `notes` more, for a document with empty property
  containers.
- A script that relied on `replace` or `edit` of a .docx refusing without `-o` now gets a new file beside the input.
- The others line costs one `revisions` pass over the document per command.
