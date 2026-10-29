# 0013. The MCP server steers to one call per document and costs fewer tokens per turn

- **Date:** 2026-10-29
- **Status:** accepted on 2026-10-29

## Context

On Adeu's docx-benchmark (gemini-3.5-flash, 5 scenarios, 15 runs each), stetdoc completed 75 of 75 tasks, as Adeu did,
but took 21.7 turns and an estimated $0.96 a task against Adeu's 15.5 turns and $0.71. Output tokens were level, so the
gap was input: every turn sends the whole conversation and the tool list again. The calls behind it, per task, were
`check` after a write (1.63 calls), an `unpack` and `pack` round trip the edit did not need (1.87), a dry run before an
all-or-nothing batch (1.36), failed calls (1.07, half of them a working copy passed by its bare name from another
folder), and receipts of about 10 KB, since each result was the pretty-printed JSON. The 12 tools' list was about 7,200
tokens (characters divided by 4), against about 4,100 for Adeu's 6, and `edit` alone was 12,500 characters.

Two of the fixes reach the core's public interface, so this records them, with the server's changes they serve.

## Decision

- **A result is the text the CLI prints.** It is what the skill teaches and about a tenth of the JSON for a typical
  receipt. `structuredContent` still carries the JSON whole. The MCP specification says a tool that returns structured
  content should also return it serialized in a text block; the text is the same information, read as the skill
  describes it, and a client that wants the fields has them in `structuredContent`.
- **The words are the tool's.** A next step or a receipt that names a CLI flag the tool has says the property instead:
  `--all` becomes `all: true`, `--range` becomes `range`, `-o` becomes `output`.
- **A receipt says its check, and is short.** Every receipt's check line says the verdict and that `check` need not run:
  `validated 3 units: no problems, and this file needs no separate check.` The writing tools cut a receipt's text to about
  2 KB with **`Receipt::text_within(cap)`**, a new core method that composes the text as `text` is composed, in `cap`
  bytes: the head line with its base, the first operations and blocks, what was left out and the check line. `detail:
  true` returns the whole text. The CLI's text is unchanged.
- **The descriptions steer to the direct path.** Each tool's description says the few words a model needs: one
  `replace`, `comment` or `edit` call on the .docx with every change of the document, all or none; no `unpack`, `pack`,
  `check` or dry run needed, since a refusal writes nothing and says how to retry. The detail moves to `skills_get` and
  to the refusals. The edit tool's typed operations keep every field, with a description only where the name does not
  say enough. The list is now 11 tools and 16,419 characters (about 4,100 tokens), from 12 and 28,693.
- **Fewer failed calls.** A relative path to read that is not there as given is looked for in the folders of the
  documents the session has named or written, most recent first; a path to write never moves, and `--root` still
  confines every path. A boolean argument, or a typed operation's boolean or number field, may come as text. `view`
  takes `find` with `story`, through **`Scope::Find { story }`**, a new field (`None` by default, and in JSON) that limits
  a find to one story. An `ambiguous_match` refusal of `replace` or `comment` says to pass `all: true` for every place or
  `range` with one place's ref, such as `range p12..p12`.
- **Several comments in one call.** `comment` takes `ANCHOR TEXT` pairs as `replace` takes `OLD NEW` pairs, and the MCP
  tool takes them in `comments`.

### Which tools

We kept 11 of the 12 tools and dropped one. `capabilities` is not a tool any more: it lists the operations and the
error codes, which `skills_get` teaches with what to do, and the JSON Schema of the public types, which serves programs.
The CLI keeps it. The edit tool also leaves out `offsets`, which serves programs that count in UTF-16; an agent counts
characters, and the CLI keeps the flag.

We considered merging further and did not:

- **`unpack`, `pack` and `check` into one tool with an action.** Their arguments differ, so the merged schema would be
  the union of the three with a field that says which apply. Gemini flattens such unions, and a model sends the wrong
  arguments for the action more often. The saving is about 300 characters, since a tool's own overhead is small: the
  bytes were in the descriptions. The working copy also stays the way to edit readable files with an agent's own tools.
- **`lookup` into `inspect`.** Both take a document and a ref, with different meanings for the ref, so one tool would
  have to guess which was meant.
- **`replace` and `comment` into `edit`.** `edit` takes refs from a view, and `replace` and `comment` find their text
  themselves. Merging them would make every edit's schema carry both ways, and the two that need no refs are the
  cheapest path for most tasks.

## Consequences

- A client that parsed the JSON from a result's text must read `structuredContent` instead.
- A receipt's text over MCP is a summary for a large batch. The JSON still has every operation, and `detail: true` the
  whole text.
- The check line of every receipt is longer by about 40 bytes.
- A caller that built `Scope::Find` with a struct literal adds `story: None`.
- A relative path that is not there as given may resolve to a file of that name in a folder the session has seen. Only
  paths to read are looked for, and the error for a name that is nowhere is unchanged.

## Amendment, 2026-10-29: the check line reads as done, and success says no "error"

A one-run benchmark of the build above still called `check` 1.8 times a task. Of its 9 calls, 6 came at the end of a
task, after the last write and a view, as a final verification. The harness's system prompt mentions a "validation
gate" before submission. The other 3 checked the input, 2 of them after the agent read the core skill, whose workflow
listed "Check" as step 4. So the receipt's line now reads as a completed validation: `validated 3 units: no problems,
and this file needs no separate check.` The `check` tool is described as only for a .docx stetdoc did not write or for
hand edits of a working copy. The skill's step 4 says the same, and that a document you only read needs no check either.

The same run's harness counted two `skills_get` results as failed calls, because it greps a result's text for
`"error"`. The core skill no longer carries the table of refusal codes, which `skills get errors` has. It says "refusal"
where it said "error" in its prose, and it no longer quotes `"error"`. Real refusals keep `error[code]`.
