# CLI reference

Generated from the `stetdoc` binary's own definitions by `cargo xtask docs`: do not edit it by hand. The command and flag descriptions are the `--help` text, and every example runs as a test (`crates/stetdoc-cli/doc/`). The workflow and the rules for agents are in the skill: `stetdoc skills get core`.

Every command takes `--json`, which prints the result, or the error, as JSON on stdout. Data goes to stdout and diagnostics to stderr. Document text never goes in an argument: batches come from stdin or `--ops`.

## Commands

| Command | Does |
|---|---|
| `stetdoc unpack` | Write a working copy of a .docx: readable files beside a .stet/ folder that you never touch |
| `stetdoc pack` | Compile the edits of a working copy into tracked changes, validate them and write the .docx |
| `stetdoc view` | Show a document or a working copy: an outline of a large document, or a range, a search, the revisions, the comments |
| `stetdoc inspect` | Show the effective formatting of a block and where each value comes from, or the styles, lists or sections |
| `stetdoc edit` | Apply a batch of operations, read from stdin or --ops, to a working copy or to a .docx with -o |
| `stetdoc compare` | Write a redline of the differences between two documents as tracked changes |
| `stetdoc check` | Validate, and show every revision that pack would write, by author |
| `stetdoc lookup` | Find a defined term, a cross-reference, or what refers to a clause |
| `stetdoc skills` | Print the agent skill embedded in this binary |
| `stetdoc init` | Install the skill stub for the agents you pick, in your home folder or (with --project) in a project |
| `stetdoc capabilities` | List the operations, whether each is tracked, the error codes and the format version |
| `stetdoc mcp` | Serve the commands as tools over the Model Context Protocol, on stdin and stdout |

## `stetdoc unpack`

Write a working copy of a .docx: readable files beside a .stet/ folder that you never touch.

```text
stetdoc unpack [OPTIONS] <INPUT>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | The .docx to unpack, or with --refresh a working copy to render again. |
| `-o, --output <DIR>` | The working copy to write. Defaults to NAME.stet beside the .docx. |
| `--refresh` | Render every readable file of an existing working copy again, discarding hand edits that were not packed. |

### Examples

Unpack a document once, and keep the folder for the whole task.

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

```

## `stetdoc pack`

Compile the edits of a working copy into tracked changes, validate them and write the .docx.

```text
stetdoc pack [OPTIONS] <DIR>
```

| Argument | Meaning |
|---|---|
| `<DIR>` | The working copy. |
| `-o, --output <DOCX>` | The .docx to write. Defaults to NAME.edited.docx beside the working copy. An existing file is overwritten only when it is the document the working copy came from or one it packed. |
| `--force` | Overwrite the output even when it holds a document this working copy neither came from nor packed. |
| `--no-track` | Write the edits as plain changes, not tracked ones. Only for documents that are not under review. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

`pack` takes in the hand edits of a working copy, writes `NAME.edited.docx` beside it unless `-o` names another file, and
brings the working copy up to date. It overwrites an existing file only when that file is the document the working copy
came from or one it packed, so a newer version that someone else saved is never lost: `--force` overrides that.

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ stetdoc pack contract.stet --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit.
wrote contract.edited.docx
working copy contract.stet is at base 5737b772923cf8d3:0:5737b772923cf8d3

$ stetdoc pack contract.stet -o contract.docx --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit.
wrote contract.docx
working copy contract.stet is at base 5737b772923cf8d3:0:5737b772923cf8d3

$ stetdoc pack contract.stet -o revised.docx --author "Jane Doe"
? 4
error[stale_target]: revised.docx exists and is neither the document contract.stet came from nor one it packed, so packing would overwrite a version it has not seen.
next: Unpack revised.docx again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.

```

## `stetdoc view`

Show a document or a working copy: an outline of a large document, or a range, a search, the revisions, the comments.

```text
stetdoc view [OPTIONS] <INPUT>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A .docx or a working copy. A working copy shows its last export, not edits that were not packed. |
| `--outline` | The headings only. |
| `--depth <N>` | With --outline, the deepest heading level shown. |
| `--range <REFS>` | The blocks from one ref to another, such as p40..p60, or from a ref to the end of its story. |
| `--clause <NUMBER>` | The block a clause number resolves to, with the blocks under it, such as 12.3. |
| `--find <TEXT>` | The blocks that contain this text, with their neighbours, paged like a range. Case-insensitive unless it has a capital letter. With --range, only in that range; with --revisions, only the changes whose text contains it. |
| `--context <N>` | With --find, how many blocks to show on each side of a match. Default `1`. |
| `--revisions` | The tracked changes, one line each: their ids, kind, author, day, block and text, cut to 80 characters. |
| `--full` | With --revisions, the whole inserted and deleted text of each change (up to 1,000 characters), not 80. |
| `--comments` | The comment threads. |
| `--author <NAME>` | With --revisions or --comments, only those by this author (a name or the code the view shows). |
| `--kind <KIND>` | With --revisions, only this kind: insert, delete, move or format. |
| `--resolved` | With --comments, only resolved threads. |
| `--open` | With --comments, only open threads. |
| `--story <STORY>` | One story whole. One of `body`, `notes`, `headers`, `comments`. |
| `--mode <MODE>` | How tracked changes appear. One of `final`, `original`, `redline`. Default `redline`. |
| `--budget <TOKENS>` | The size of a page, in estimated tokens (about 4 characters each). 0 shows everything. Default `3000`. |
| `--all` | Everything on one page, however long: the same as --budget 0, and it wins over --budget. |
| `--after <CURSOR>` | Continue after this cursor, from the last line of the previous page. |
| `--no-notes` | Leave out the margin notes and the appendix of the notes and comments the page cites. |

### Examples

With no flag, a view shows the whole body if it fits the budget, and its outline otherwise.

```console
$ stetdoc view contract.docx --find "thirty days" --context 0
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{~ 1 match}
{position: p4..p4 of find "thirty days": 1 match in 1 block (1 of 1 blocks)}

```

A find pages like a range, and its last line counts the matches. With `--range` it looks only in that range. With
`--revisions` it keeps the changes whose inserted or deleted text holds it, and `--full` shows each change's whole text.
`--all` is `--budget 0`: everything on one page.

```console
$ stetdoc view contract.docx --find "days" --range p3..p5 --context 0
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{~ 1 match}
{position: p4..p4 of find "days" in p3..p5: 1 match in 1 block (1 of 1 blocks)}

$ stetdoc view tracked.docx --revisions --find law --full
{stet 1 view=ledger base=676e65f4e88a38bd:0:676e65f4e88a38bd mode=redline common-style="Normal" range=change:4..change:4}
{authors: EW "Eric White"}
{summary: 4 tracked changes (JD 2, EW 2) · 1 section}
{changes: 1 change}
#4 insert EW 2026-01-05 p4 "English law"
{position: change:4..change:4 of changes with text "law" (1 of 1 changes)}

```

## `stetdoc inspect`

Show the effective formatting of a block and where each value comes from, or the styles, lists or sections.

```text
stetdoc inspect [OPTIONS] <INPUT> <TARGET>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A .docx or a working copy. |
| `<TARGET>` | A ref (p42, t3, r17), or styles, numbering or sections. |

### Examples

```console
$ stetdoc inspect contract.docx p1
block: p1 is a paragraph in the body story
characters 0..67: plain
tracked changes: none in this block: all of its text is the document's own, even text that looks like a revision
comments: none on this block

```

A block lists its tracked changes and comments, or says it has none, so text that only looks like a change can be told
from one.

```console
$ stetdoc inspect tracked.docx p2
block: p2 is a paragraph in the body story
characters 0..10: plain
change #2: delete by Eric White on 2026-01-03, deletes "gross"
comments: none on this block

```

## `stetdoc edit`

Apply a batch of operations, read from stdin or --ops, to a working copy or to a .docx with -o.

```text
stetdoc edit [OPTIONS] <INPUT>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A working copy, or a .docx with -o. |
| `--ops <FILE>` | The batch, as block text or JSON. Defaults to stdin; - is stdin too. |
| `-o, --output <DOCX>` | With a .docx, the .docx to write. A package is never edited in place. |
| `--no-track` | Write plain changes, not tracked ones. Only for documents that are not under review. |
| `--dry-run` | Validate and show the receipt, and write nothing. |
| `--scope <REFS>` | Refuse the whole batch if it changes anything outside these refs and ranges, such as p12,p40..p60. |
| `--quotes <QUOTES>` | Follow the paragraph's quotes (match), or keep them as typed. One of `match`, `as-typed`. Default `match`. |
| `--offsets <OFFSETS>` | What offsets count: Unicode scalar values, or UTF-16 units as JavaScript does. One of `scalar`, `utf16`. Default `scalar`. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

A batch in block text, here in `batch.txt`. An agent usually sends it on stdin with a quoted heredoc
(`stetdoc edit contract.stet <<'EOF'` ... `EOF`), so no text needs escaping.

```text
base: 5737b772923cf8d3:0:5737b772923cf8d3
---
@replace
ref: p4
find: thirty days
with: sixty days
```

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ stetdoc edit contract.stet --author "Jane Doe" --ops batch.txt
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:1:6a07baa2d488eeb5
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 1 unit.

```

The same batch on a .docx writes another .docx, and `--dry-run` writes nothing.

```console
$ stetdoc edit contract.docx -o edited.docx --author "Jane Doe" --ops batch.txt --dry-run
dry run: nothing was applied. A new block has no ref until the edit is applied. 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:0:5737b772923cf8d3
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 1 unit.

```

## `stetdoc compare`

Write a redline of the differences between two documents as tracked changes.

```text
stetdoc compare [OPTIONS] --output <DOCX> <ORIGINAL> <REVISED>
```

| Argument | Meaning |
|---|---|
| `<ORIGINAL>` | The original document. |
| `<REVISED>` | The revised document. |
| `-o, --output <DOCX>` | The redline to write. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

`compare` writes the redline of two documents: the revised one, with what changed from the original as tracked changes.

```console
$ stetdoc compare contract.docx revised.docx -o redline.docx --author "Jane Doe"
compared: 3 revisions, tracked as Jane Doe 2026-10-06T09:30:00Z. base 3953ae684513612a:1:b1378820043c0bb8
Jane Doe: insertion 2, deletion 1
checked 2 units.
wrote redline.docx

```

## `stetdoc check`

Validate, and show every revision that pack would write, by author.

```text
stetdoc check [OPTIONS] <INPUT>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A working copy or a .docx. |
| `--strict` | Fail (exit 5) on any violation, including ones already in the input. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

`check` shows what pack would write. With hand edits pending, it needs the author.

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ stetdoc check contract.stet
No pending edits.

Validation: no violations in 1 part.

```

## `stetdoc lookup`

Find a defined term, a cross-reference, or what refers to a clause.

```text
stetdoc lookup [OPTIONS] <INPUT> <QUERY>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A .docx or a working copy. |
| `<QUERY>` | A ref, a clause number or a defined term. |

### Examples

```console
$ stetdoc lookup contract.docx Losses
{stet 1 view=lookup base=5737b772923cf8d3:0:5737b772923cf8d3 query="Losses"}
{term "Losses" · exact · 1 definition · 0 uses}
  defined p2 (definition): “Losses” means any loss, and the Supplier’s liability.

```

## `stetdoc skills`

Print the agent skill embedded in this binary.

```text
stetdoc skills [OPTIONS] <COMMAND>
```


`stetdoc skills get [OPTIONS] [NAME]`: Print a skill: core (the default), operations or errors.
- `<NAME>`: The skill's name.
- `--full`: Print every part of the skill.
- `--json`: Print the result, or the error, as JSON on stdout.

### Examples

`core` is what an agent loads first; `operations` and `errors` are its references, and `--full` prints all three.

```console
$ stetdoc skills get core
# stetdoc: editing Word documents with tracked changes
...
```

## `stetdoc init`

Install the skill stub for the agents you pick, in your home folder or (with --project) in a project.

```text
stetdoc init [OPTIONS]
```

| Argument | Meaning |
|---|---|
| `--project` | Install into a project instead of your home folder, and write the AGENTS.md block there. |
| `--dir <DIR>` | With --project, the project's folder. Defaults to the current directory. |
| `--agent <AGENT>` | The agents to install for, such as claude-code,codex, or all. Without it, init asks in a terminal and otherwise installs for the agents it detects. |
| `--list` | List the agents, whether each is detected, its skill folder, and the stetdoc version installed there. |
| `--remove` | Remove the skill folders stetdoc wrote, and with --project and no --agent the AGENTS.md block. Anything else is left alone. |
| `--mcp` | Also print, for each agent, the command or config snippet that registers `stetdoc mcp`. Nothing is edited. |
| `--author <NAME>` | Save NAME as the author of your revisions in the config file. Without it, init asks in a terminal when no name is saved, suggesting your git user.name. |
| `--dry-run` | List what would be written or removed, and change nothing. |
| `--force` | Replace a SKILL.md that is not an unchanged stetdoc stub, or is a link. Without it, init refuses and changes nothing. |

### Examples

Without --project, init installs into your home folder, for the agents you pick at the prompt or name with --agent, or
for the ones it detects. `stetdoc init --list` shows every agent and what is installed.

```console
$ stetdoc init --project --agent codex,claude-code
note: no author is saved, and every revision needs one. Run stetdoc init --author "Your Name", or set STETDOC_AUTHOR, before you edit.
wrote .agents/skills/stetdoc/SKILL.md (Codex)
wrote .claude/skills/stetdoc/SKILL.md (Claude Code)
wrote AGENTS.md

```

## `stetdoc capabilities`

List the operations, whether each is tracked, the error codes and the format version.

```text
stetdoc capabilities [OPTIONS]
```

| Argument | Meaning |
|---|---|
| `--schema` | Print the JSON Schema of every public type instead. |

### Examples

`--json` prints the same as data, and `--schema` the JSON Schema of every public type.

```console
$ stetdoc capabilities
stetdoc 1.0.0-dev
formats: stet/1 (.md)
complement: 1.0

operations:
  replace         tracked    Replaces a text range, or the whole paragraph, with new text. The difference is made word by word.
  insert          tracked    Inserts new paragraphs before or after a paragraph or a table, in order.
  delete          tracked    Deletes a whole paragraph or table, a text range, or the blocks from one paragraph through another.
  split           tracked    Cuts a paragraph in two, at an offset or before or after some text.
  merge           tracked    Joins a paragraph and the one after it.
  format          tracked    Changes the formatting of the runs in a range, or of the whole paragraph.
  paragraph       tracked    Changes the formatting of a paragraph. Values take units such as 0.5in, 6pt and 1.5cm.
  style           tracked    Sets the paragraph style.
  list            tracked    Changes the level of a list item, restarts its numbering at a number, or takes it out of the list.
  set             tracked    Sets any schema property of a paragraph, run, paragraph mark, section, table, row or cell.
  row             tracked    Inserts a table row, or deletes one.
  comment         untracked  Adds a comment anchored to a range, or to the whole paragraph.
  reply           untracked  Replies to a comment thread.
  resolve         untracked  Resolves a comment thread, or reopens it.
  edit-comment    untracked  Replaces the text of one of the author's own comments.
  delete-comment  untracked  Deletes one of the author's own comments, with its replies.
  delete-note     tracked    Deletes a footnote or endnote, with its reference.
  accept          untracked  Accepts revisions: applies them.
  reject          untracked  Rejects revisions: undoes them.

error codes:
  unsupported          exit 1
  cross_block          exit 1
  readonly_region      exit 1
  scope_violation      exit 1
  io_error             exit 1
  invalid_payload      exit 2
  not_found            exit 3
  ambiguous_match      exit 3
  stale_ref            exit 4
  stale_files          exit 4
  pending_edits        exit 4
  stale_target         exit 4
  validation_failed    exit 5
  malformed_input      exit 6
  limit_exceeded       exit 6
  corrupt_complement   exit 6
  unsupported_version  exit 6
  internal             exit 7

```

## `stetdoc mcp`

Serve the commands as tools over the Model Context Protocol, on stdin and stdout.

```text
stetdoc mcp [OPTIONS]
```

| Argument | Meaning |
|---|---|
| `--root <DIR>` | Confine every path argument to this folder, and resolve relative paths against it. Without it, relative paths resolve against the current directory and any path is allowed. |
| `--author <NAME>` | The author of the revisions of every call that writes them and names none, ahead of STETDOC_AUTHOR and the name saved with stetdoc init. |

### Examples

An agent's MCP configuration starts the server; `stetdoc init --mcp` prints the line for each agent. Each tool is a command
with its flags as arguments, and answers with what the command prints with `--json`. The edit tool takes its batch in
`batch`. A session, one message per line:

```text
> {"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"unpack","arguments":{"input":"contract.docx"}}}
< {"jsonrpc":"2.0","id":1,"result":{"content":[...],"structuredContent":{"base":"...","files":[...],"working_copy":"contract.stet"},"isError":false,...}}
```

Without `--root`, the server reads and writes any file the user who runs it can, as the CLI does. With `--root`, every path
argument must stay inside that folder: a `..` or a symbolic link that leads out is refused. The check runs before each
command, so the root must be a folder only the server writes: a process that can write there could swap a link in
between the check and the command. Files of a working copy's `.stet/` that are links are refused too.

```console
$ stetdoc mcp --root nowhere
? 2
error[invalid_payload]: --root nowhere: No such file or directory (os error 2).
next: Name an existing folder with --root.

```

## Exit codes

The exit status is the class of the error's code. The codes are in the [error reference](errors.md).

| Exit | Meaning | Codes |
|---|---|---|
| 0 | success | |
| 1 | operation failed | `unsupported`, `cross_block`, `readonly_region`, `scope_violation`, `io_error` |
| 2 | usage | `invalid_payload`, and argument errors |
| 3 | target | `not_found`, `ambiguous_match` |
| 4 | stale | `stale_ref`, `stale_files`, `pending_edits`, `stale_target` |
| 5 | validation | `validation_failed`, and `check --strict` with any violation |
| 6 | input | `malformed_input`, `limit_exceeded`, `corrupt_complement`, `unsupported_version` |
| 7 | internal | `internal` |

## JSON output

With `--json`, a command prints one JSON value on stdout. Field names are snake_case, fields may be added, and none is removed before 2.0. The core's types are described by `stetdoc capabilities --schema`.

| Command | Prints |
|---|---|
| `view`, `inspect`, `lookup`, `check`, `capabilities` | `View`, `Inspection`, `Lookup`, `CheckResult`, `Capabilities` |
| `edit` on a working copy | `Receipt` |
| `edit -o`, `compare` | `{ output, receipt, saved }` |
| `pack` | `{ output, receipt (or null), saved, working_copy }` |
| `unpack` | `{ working_copy, base, files }` |
| `skills get` | `{ name, text }`, or `{ parts }` with `--full` |
| `init` | `{ scope, root, dry_run, agents, written, unchanged, author }`, and `mcp` with `--mcp`; `--list` gives `{ scope, root, agents, author, config }` and `--remove` `{ scope, root, dry_run, removed, agents_md }` |
| any failure | `{ "error": { code, message, next, op_index, file, line } }`, the last three when known |

## Environment

| Variable | Meaning |
|---|---|
| `STETDOC_AUTHOR` | The author of revisions when `--author` is absent. Then comes the name saved with `stetdoc init` in `stetdoc/config.toml` (under `XDG_CONFIG_HOME`, else `%APPDATA%` on Windows, else `~/.config`). There is no other default. |
| `XDG_CONFIG_HOME` | Where the config file is, as above. |
| `STETDOC_DATE` | The date of revisions when `--date` is absent, `YYYY-MM-DDTHH:MM:SS`. Otherwise the local clock. |
| `TZ` | The zone of the local clock: an IANA name such as `Asia/Kolkata`, a POSIX rule, or a path to zone data. Otherwise the system's zone. A zone stetdoc cannot read gives UTC, with a warning. |
| `NO_COLOR` | Respected. stetdoc prints no colour of its own. |
