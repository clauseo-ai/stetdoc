<!-- Generated from the tool definitions of `stetdoc mcp` by `cargo xtask docs`: do not edit it by hand. -->

# MCP tools

Each tool is a command of the [CLI](cli.md), and each description is the one `tools/list` sends. Every tool also returns its result as JSON in `structuredContent`.

| Tool | Description | Arguments |
|---|---|---|
| [`unpack`](#unpack) | Write a working copy of a .docx. Not needed to edit: replace, comment and edit change a .docx in one call. Unpack only to edit the files with your own tools, then pack. | `input` (required), `output`, `refresh` |
| [`pack`](#pack) | Write a working copy's edits into the .docx as tracked changes, validated. On stale_target, unpack again rather than force. | `dir` (required), `author`, `date`, `detail`, `done`, `force`, `no_track`, `output` |
| [`view`](#view) | Read a .docx: pages of about 3000 tokens (pass after: the page's next), the outline of a long body, a range, clauses, revisions, comments, or find: a line per matching paragraph with its ref, the cheap way to locate text. Use one of outline, range, clause, revisions, comments, story; find also goes with range, story or revisions. | `input` (required), `after`, `all`, `author`, `budget`, `clause`, `comments`, `context`, `depth`, `find`, `full`, `kind`, `mode`, `no_notes`, `open`, `outline`, `range`, `resolved`, `revisions`, `story` |
| [`inspect`](#inspect) | Show a block's effective formatting and where it comes from, or the styles, numbering or sections. | `input` (required), `target` (required) |
| [`edit`](#edit) | Change a .docx in one call: all its operations and comments together, applied all or none. A refused call changes nothing and says why, so never dry-run first. Refs (p42, t3, c3) come from view. A .docx needs no base, unpack, pack or check: it goes to NAME.edited.docx unless output or in_place says where, and it is validated as written. Op fields: skills_get name operations. For text changes or comments, replace and comment need no refs. | `input` (required), `author`, `base`, `batch`, `date`, `detail`, `dry_run`, `in_place`, `manual`, `manual_color`, `no_track`, `ops`, `ops_file`, `output`, `quotes`, `scope` |
| [`replace`](#replace) | Replace text in a .docx as tracked changes in one call, no refs needed: put every change in pairs. Each old must be in the document once, or nothing is written and the error lists each place: retry with all: true for every one, a range, or a longer old. Empty new deletes. Goes to NAME.edited.docx unless output or in_place; it is validated as written, so no check or dry run is needed. | `input` (required), `pairs` (required), `all`, `author`, `clause`, `date`, `detail`, `dry_run`, `expect`, `ignore_case`, `in_place`, `manual`, `manual_color`, `output`, `quotes`, `range` |
| [`comment`](#comment) | Comment on text in a .docx in one call, no refs needed: anchor and text for one, comments for several, all or none. Each anchor must be there once, as for replace (all: true for every one). Goes to NAME.edited.docx unless output or in_place. To reply or resolve, use edit. | `input` (required), `all`, `anchor`, `author`, `clause`, `comments`, `date`, `detail`, `dry_run`, `expect`, `ignore_case`, `in_place`, `output`, `quotes`, `range`, `text` |
| [`compare`](#compare) | Write a redline of two documents as tracked changes. | `original` (required), `output` (required), `revised` (required), `author`, `date` |
| [`check`](#check) | Only for a .docx stetdoc did not write, or hand edits of a working copy: stetdoc validates what it writes. | `input` (required), `all`, `author`, `date`, `strict` |
| [`lookup`](#lookup) | Find a defined term, a cross-reference, or what refers to a clause. | `input` (required), `query` (required) |
| [`skills_get`](#skills_get) | The guide: core (workflow, rules), operations (op fields), errors (what each code means). | `full`, `name` |

## `unpack`

Write a working copy of a .docx. Not needed to edit: replace, comment and edit change a .docx in one call. Unpack only to edit the files with your own tools, then pack.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or with refresh a working copy. |
| `output` | string | no | The working copy to write, instead of NAME.stet beside the .docx. |
| `refresh` | boolean | no | Render a working copy again, dropping unpacked hand edits. |

## `pack`

Write a working copy's edits into the .docx as tracked changes, validated. On stale_target, unpack again rather than force.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `dir` | string | yes | The working copy. |
| `author` | string | no | The author, if not the server's. |
| `date` | string | no | YYYY-MM-DDTHH:MM:SS. |
| `detail` | boolean | no | The whole receipt, not a summary of about 2 KB. |
| `done` | boolean | no | Remove the working copy once the .docx is written. |
| `force` | boolean | no | Overwrite an output this working copy did not write. |
| `no_track` | boolean | no | Untracked: only for a document not under review. |
| `output` | string | no | The .docx to write, instead of NAME.edited.docx. |

## `view`

Read a .docx: pages of about 3000 tokens (pass after: the page's next), the outline of a long body, a range, clauses, revisions, comments, or find: a line per matching paragraph with its ref, the cheap way to locate text. Use one of outline, range, clause, revisions, comments, story; find also goes with range, story or revisions.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `after` | string | no | The next of the previous page. |
| `all` | boolean | no | Everything on one page. |
| `author` | string | no | With revisions or comments: only this author. |
| `budget` | integer | no | The page size in tokens; 0 is everything. |
| `clause` | array of strings | no | The blocks of these clauses, such as 12.3. |
| `comments` | boolean | no | The comment threads. |
| `context` | integer | no | With find or revisions: whole blocks, with N on each side. |
| `depth` | integer | no | With outline: the deepest level. |
| `find` | array of strings | no | Paragraphs holding any of these texts, one line each with its ref. |
| `full` | boolean | no | With revisions: the whole changed text. |
| `kind` | string | no | With revisions: insert, delete, move or format. |
| `mode` | one of `final`, `original`, `redline` | no | How tracked changes show. Default `redline`. |
| `no_notes` | boolean | no | Leave out the margin notes. |
| `open` | boolean | no | With comments: only open threads. |
| `outline` | boolean | no | The headings only. |
| `range` | string | no | The blocks from one ref to another, such as p40..p60. |
| `resolved` | boolean | no | With comments: only resolved threads. |
| `revisions` | boolean | no | The tracked changes, one line each. |
| `story` | one of `body`, `notes`, `headers`, `comments` | no | One story whole, or with find, only its matches. |

## `inspect`

Show a block's effective formatting and where it comes from, or the styles, numbering or sections.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `target` | string | yes | A ref (p42, t3, r17), or styles, numbering or sections. |

## `edit`

Change a .docx in one call: all its operations and comments together, applied all or none. A refused call changes nothing and says why, so never dry-run first. Refs (p42, t3, c3) come from view. A .docx needs no base, unpack, pack or check: it goes to NAME.edited.docx unless output or in_place says where, and it is validated as written. Op fields: skills_get name operations. For text changes or comments, replace and comment need no refs.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `author` | string | no | The author, if not the server's. |
| `base` | string | no | The base token of the view or receipt the refs come from: for a working copy, not a .docx. |
| `batch` | string | no | The operations as block text or JSON, instead of ops. |
| `date` | string | no | YYYY-MM-DDTHH:MM:SS. |
| `detail` | boolean | no | The whole receipt, not a summary of about 2 KB. |
| `dry_run` | boolean | no | Write nothing. Rarely needed: a refusal writes nothing. |
| `in_place` | boolean | no | Write back to the input: only a file the user asked to update or this task wrote. |
| `manual` | boolean | no | Strike and underline, not tracked: only if the user asks. |
| `manual_color` | boolean | no | As manual, in red and blue. |
| `no_track` | boolean | no | Untracked: only for a document not under review. |
| `ops` | array of objects | no | The operations, in order, all applied or none. |
| `ops_file` | string | no | A file holding the batch, instead of ops. |
| `output` | string | no | The .docx to write, instead of NAME.edited.docx. |
| `quotes` | one of `match`, `as-typed` | no | Quotes. Default `match`. |
| `scope` | array of strings | no | Refuse the batch if it changes anything outside these refs, such as p40..p60. |

## `replace`

Replace text in a .docx as tracked changes in one call, no refs needed: put every change in pairs. Each old must be in the document once, or nothing is written and the error lists each place: retry with all: true for every one, a range, or a longer old. Empty new deletes. Goes to NAME.edited.docx unless output or in_place; it is validated as written, so no check or dry run is needed.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `pairs` | array of `{new, old}` | yes | Each old text and the new text it becomes. |
| `all` | boolean | no | Replace every occurrence of each old. |
| `author` | string | no | The author, if not the server's. |
| `clause` | array of strings | no | Look only in these clauses, such as 11.1. |
| `date` | string | no | YYYY-MM-DDTHH:MM:SS. |
| `detail` | boolean | no | The whole receipt, not a summary of about 2 KB. |
| `dry_run` | boolean | no | Write nothing. Rarely needed: a refusal writes nothing. |
| `expect` | integer | no | Every occurrence, only if there are exactly N. |
| `ignore_case` | boolean | no | Match whatever the case. |
| `in_place` | boolean | no | Write back to the input: only a file the user asked to update or this task wrote. |
| `manual` | boolean | no | Strike and underline, not tracked: only if the user asks. |
| `manual_color` | boolean | no | As manual, in red and blue. |
| `output` | string | no | The .docx to write, instead of NAME.edited.docx. |
| `quotes` | one of `match`, `as-typed` | no | Quotes. Default `match`. |
| `range` | string | no | Look only in these blocks, such as p40..p60. |

## `comment`

Comment on text in a .docx in one call, no refs needed: anchor and text for one, comments for several, all or none. Each anchor must be there once, as for replace (all: true for every one). Goes to NAME.edited.docx unless output or in_place. To reply or resolve, use edit.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `all` | boolean | no | Comment on every occurrence of each anchor. |
| `anchor` | string | no | The text to comment on. |
| `author` | string | no | The author, if not the server's. |
| `clause` | array of strings | no | Look only in these clauses, such as 11.1. |
| `comments` | array of `{anchor, text}` | no | Several comments, each an anchor and a text. |
| `date` | string | no | YYYY-MM-DDTHH:MM:SS. |
| `detail` | boolean | no | The whole receipt, not a summary of about 2 KB. |
| `dry_run` | boolean | no | Write nothing. Rarely needed: a refusal writes nothing. |
| `expect` | integer | no | Every occurrence, only if there are exactly N. |
| `ignore_case` | boolean | no | Match whatever the case. |
| `in_place` | boolean | no | Write back to the input: only a file the user asked to update or this task wrote. |
| `output` | string | no | The .docx to write, instead of NAME.edited.docx. |
| `quotes` | one of `match`, `as-typed` | no | Quotes. Default `match`. |
| `range` | string | no | Look only in these blocks, such as p40..p60. |
| `text` | string | no | The comment. |

## `compare`

Write a redline of two documents as tracked changes.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `original` | string | yes | The original document. |
| `output` | string | yes | The redline to write. |
| `revised` | string | yes | The revised document. |
| `author` | string | no | The author, if not the server's. |
| `date` | string | no | YYYY-MM-DDTHH:MM:SS. |

## `check`

Only for a .docx stetdoc did not write, or hand edits of a working copy: stetdoc validates what it writes.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `all` | boolean | no | List up to 200 findings, not 20. |
| `author` | string | no | The author, if not the server's. |
| `date` | string | no | YYYY-MM-DDTHH:MM:SS. |
| `strict` | boolean | no | Fail on any violation, old ones too. |

## `lookup`

Find a defined term, a cross-reference, or what refers to a clause.

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `input` | string | yes | The .docx, or a working copy. |
| `query` | string | yes | A ref, a clause number or a defined term. |

## `skills_get`

The guide: core (workflow, rules), operations (op fields), errors (what each code means).

| Argument | Type | Required | Meaning |
|---|---|---|---|
| `full` | boolean | no | Every part of the skill. |
| `name` | string | no | core (the default), operations or errors. |
