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
| `stetdoc edit` | Apply a batch of operations, read from stdin or --ops, to a working copy or a .docx |
| `stetdoc replace` | Replace text in one step, as tracked changes or, with --manual, as manual markup: each OLD, found once in the document as it reads now, becomes NEW |
| `stetdoc comment` | Comment on text in one step: the comment is anchored to ANCHOR, found once in the document as it reads now |
| `stetdoc compare` | Write a redline of the differences between two documents as tracked changes |
| `stetdoc check` | Validate, and show every revision that pack would write, by author |
| `stetdoc lookup` | Find a defined term, a cross-reference, or what refers to a clause |
| `stetdoc skills` | Print the agent skill embedded in this binary |
| `stetdoc init` | Install the skill stub for the agents you pick, in your home folder or (with --project) in a project |
| `stetdoc capabilities` | List the operations, whether each is tracked, the refusal codes and the format version |
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
| `--done` | Remove the working copy once the .docx is written and validated. When the pack fails, the working copy stays. Files that stetdoc did not write stay too, and so does the folder that holds them. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

`pack` takes in the hand edits of a working copy, writes `NAME.edited.docx` beside it unless `-o` names another file, and
brings the working copy up to date. It overwrites an existing file only when that file is the document the working copy
came from or one it packed, so a newer version that someone else saved is never lost: `--force` overrides that. The
working copy stays beside the document until you delete the folder, or until a pack with `--done` succeeds and removes it.

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ stetdoc pack contract.stet --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit, which was validated.
wrote contract.edited.docx
working copy contract.stet is at base 5737b772923cf8d3:0:5737b772923cf8d3. It stays until you delete the folder, or pack again with --done to remove it.

$ stetdoc pack contract.stet -o contract.docx --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit, which was validated.
wrote contract.docx
working copy contract.stet is at base 5737b772923cf8d3:0:5737b772923cf8d3. It stays until you delete the folder, or pack again with --done to remove it.

$ stetdoc pack contract.stet -o revised.docx --author "Jane Doe"
? 4
error[stale_target]: revised.docx exists and is neither the document contract.stet came from nor one it packed, so packing would overwrite a version it has not seen.
next: Unpack revised.docx again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.

$ stetdoc pack contract.stet -o revised.docx --done --author "Jane Doe"
? 4
error[stale_target]: revised.docx exists and is neither the document contract.stet came from nor one it packed, so packing would overwrite a version it has not seen.
next: Unpack revised.docx again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.

$ stetdoc pack contract.stet --done --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit, which was validated.
wrote contract.edited.docx
removed the working copy contract.stet

$ stetdoc view contract.stet
? 3
error[not_found]: There is no file at contract.stet.
next: Check the path, and that the file is readable.

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
| `--clause <NUMBER>` | The block a clause number resolves to, with the blocks under it, such as 12.3. Repeat it to view several clauses at once: their blocks in document order, each once. |
| `--find <TEXT>` | The paragraphs that contain this text, one line each: the ref, the clause, and the text around the match, paged like a range. Case-insensitive unless it has a capital letter. Repeat it to find any of several texts: each line then says which it holds. With --range, only in that range; with --story, only in that story; with --revisions, only the changes whose text contains one of them. |
| `--context <N>` | With --find, show the matching blocks whole, with N blocks on each side, and not one line for each match. With --revisions, show the blocks that hold the changes whole, as the redline shows them, with N blocks on each side, and not one line for each change. 0 is the one-line list. Default `0`. |
| `--revisions` | The tracked changes, one line each: their ids, kind, author, day, block and text, cut to 80 characters. |
| `--full` | With --revisions, the whole inserted and deleted text of each change (up to 1,000 characters), not 80. |
| `--comments` | The comment threads. |
| `--author <NAME>` | With --revisions or --comments, only those by this author (a name or the code the view shows). |
| `--kind <KIND>` | With --revisions, only this kind: insert, delete, move or format. |
| `--resolved` | With --comments, only resolved threads. |
| `--open` | With --comments, only open threads. |
| `--story <STORY>` | One story whole, or with --find, the matches in that story only. One of `body`, `notes`, `headers`, `comments`. |
| `--mode <MODE>` | How tracked changes appear. One of `final`, `original`, `redline`. Default `redline`. |
| `--budget <TOKENS>` | The size of a page, in estimated tokens (about 4 characters each). 0 shows everything. Default `3000`. |
| `--all` | Everything on one page, however long: the same as --budget 0, and it wins over --budget. |
| `--after <CURSOR>` | Continue after this cursor, from the last line of the previous page. |
| `--no-notes` | Leave out the margin notes and the appendix of the notes and comments the page cites. |

### Examples

With no flag, a view shows the whole body if it fits the budget, and its outline otherwise.

```console
$ stetdoc view contract.docx --find "thirty days"
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{position: p4..p4 of find "thirty days": 1 match in 1 block (1 of 1 blocks)}

```

A find lists one line for each paragraph that holds the text: its ref, the clause it is in when the document numbers its
clauses, and about 100 characters of its text around the match, exactly as the paragraph has it, so a piece of it can be
an edit's `find`. An ellipsis says the paragraph goes on. Hits under the same top-level clause sit under one `{in ...}`
line that names it, and a hit on a clause heading adds a `{tip: ...}` line that names the clause to view whole. Repeat
`--find` to look for any of several texts: each line then says which texts it holds and how often, and the last line
counts each text.

```console
$ stetdoc view contract.docx --find thirty --find law --find "Due Date"
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p5}
{summary: 1 section}
p4 Payment is due within thirty days. {~ "thirty" 1}
p5 This Agreement is governed by English law. {~ "law" 1}
{position: p4..p5 of find "thirty" (1), "law" (1), "Due Date" (0): 2 matches in 2 blocks (2 of 2 blocks)}

```

`--context N` shows the matching blocks whole instead, with N blocks on each side, as an editable page would.

```console
$ stetdoc view contract.docx --find thirty --context 1
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p3..p5}
{summary: 1 section}
p3 Emoji 😀 then a tab/tand the end.
p4 Payment is due within thirty days.
{~ 1 match}
p5 This Agreement is governed by English law.
{position: p3..p5 of find "thirty": 1 match in 1 block (3 of 3 blocks)}

```

A find pages like a range, and its last line counts the matches. With `--range` it looks only in that range. With
`--revisions` it keeps the changes whose inserted or deleted text holds one of the texts, and `--full` shows each
change's whole text. `--all` is `--budget 0`: everything on one page.

```console
$ stetdoc view contract.docx --find "days" --range p3..p5
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{position: p4..p4 of find "days" in p3..p5: 1 match in 1 block (1 of 1 blocks)}

$ stetdoc view tracked.docx --revisions --find law --full
{stet 1 view=ledger base=676e65f4e88a38bd:0:676e65f4e88a38bd mode=redline common-style="Normal" range=change:4..change:4}
{authors: EW "Eric White"}
{summary: 4 tracked changes (JD 2, EW 2) · 1 section}
{changes: 1 change}
#4 insert EW 2026-01-05 p4 "English law"
{position: change:4..change:4 of changes with text "law" (1 of 1 changes)}

```

`--revisions --context N` shows the blocks that hold the changes whole, as the redline shows them, with N blocks on
each side, so each change reads in its paragraph. It takes `--author`, `--kind` and `--find` as the list does.

```console
$ stetdoc view tracked.docx --revisions --author "Eric White" --context 1
{stet 1 view=changes base=676e65f4e88a38bd:0:676e65f4e88a38bd mode=redline common-style="Normal" range=p1..p4}
{authors: JD "Jane Doe" · EW "Eric White"}
{summary: 4 tracked changes (JD 2, EW 2) · 1 section}
p1 The Seller {++shall pay ++}{#1 JD}
p2 Price {--gross--}{#2 EW} net
p3 {rev run #3 JD} Bold now
p4 Governing law: {++English law++}{#4 EW}
{position: p1..p4 of changes by Eric White: 2 changes in 2 blocks (4 of 4 blocks)}

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

Apply a batch of operations, read from stdin or --ops, to a working copy or a .docx.

```text
stetdoc edit [OPTIONS] <INPUT>
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A working copy, or a .docx. A .docx is written to NAME.edited.docx beside it, unless -o or --in-place says where. |
| `--ops <FILE>` | The batch, as block text or JSON. Defaults to stdin; - is stdin too. A batch for a .docx may leave out its base, and then its refs are read against the file as it is now. |
| `-o, --output <DOCX>` | With a .docx, the .docx to write, instead of NAME.edited.docx beside the input. It must not be the input: --in-place writes back to it. |
| `--in-place` | With a .docx, write the result back to it, atomically, and only if no one changed it during the edit. A link is written through, a hard link is broken, the permissions are kept and the owner is not; a read-only file is refused. A working copy needs no flag: an edit always writes it. |
| `--no-track` | Write plain changes, not tracked ones. Only for documents that are not under review. |
| `--manual` | Mark the changes by hand, not as tracked changes: old text stays, struck through, and new text is underlined, as plain formatting with no revision. For a reader who asked for manual markup (strikethrough and underline) and not Track Changes. A deletion is struck text only, an insertion underlined text only. |
| `--manual-color` | As --manual, with deleted text also red and new text also blue. |
| `--dry-run` | Validate and show the receipt, and write nothing. A .docx needs neither -o nor --in-place for it. |
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
validated 1 unit: no problems, and this file needs no separate check.

```

The same batch on a .docx writes another .docx with `-o`, or the same one with `--in-place`, and `--dry-run` writes nothing.
A dry run needs neither flag, and with either one it ignores it.

```console
$ stetdoc edit contract.docx -o edited.docx --author "Jane Doe" --ops batch.txt --dry-run
--dry-run writes nothing, so -o is ignored.
dry run: nothing was applied. A new block has no ref until the edit is applied. 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:0:5737b772923cf8d3
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.

```

`--in-place` writes the result back to the .docx it read: through a temporary file beside it and a rename, so a crash
leaves the old document or the new one, and only if no one wrote the file during the edit (`stale_target` otherwise).
A symbolic link is written through and stays a link. A hard link is broken, because the rename puts a new file at the
path. The permissions are kept, the owner is not, and a read-only file is refused. On Unix the folder is synced after the
rename; other systems cannot sync a folder, so there the rename is as durable as the file system makes it. A saved
document opens in a new ref namespace, so the receipt's base is the written file's: the next batch can start from
it without a view. When the edit added or removed blocks, the written file numbers them again, so the receipt has no
base and its `next` says to view the file. A batch from before the write is stale, as the second command shows.

```console
$ stetdoc edit contract.docx --in-place --author "Jane Doe" --ops batch.txt
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 745747bfd39463c1:0:745747bfd39463c1
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.
wrote contract.docx

$ stetdoc edit contract.docx --in-place --author "Jane Doe" --ops batch.txt
? 4
error[stale_ref]: This batch was written for another document (namespace 5737b772923cf8d3, not 745747bfd39463c1). Refs do not carry over when a file is reopened. View the document again. (operation 0)
next: If this file was rewritten since that view or receipt, by an edit that wrote it or by someone else, its refs are new: view it again, then rewrite the batch from what it shows.

```

## `stetdoc replace`

Replace text in one step, as tracked changes or, with --manual, as manual markup: each OLD, found once in the document as it reads now, becomes NEW.

```text
stetdoc replace [OPTIONS] <INPUT> <OLD NEW> <OLD NEW>...
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A working copy, or a .docx. A .docx is written to NAME.edited.docx beside it, unless -o or --in-place says where. |
| `<OLD NEW>` | OLD and NEW, as many pairs as you like, applied together or not at all. OLD is matched in the text as the final view reads it, across formatting, with curly and straight quotes the same; it must be there once (see --all and --expect). An empty NEW deletes OLD. Put -- before the pairs when a text starts with -. |
| `--all` | Replace every occurrence of each OLD, and say how many. |
| `--expect <N>` | Replace every occurrence of each OLD, and only if there are exactly N of each. |
| `--manual` | Mark the changes by hand, not as tracked changes: old text stays, struck through, and new text is underlined, as plain formatting with no revision. For a reader who asked for manual markup (strikethrough and underline) and not Track Changes. A deletion is struck text only, an insertion underlined text only. |
| `--manual-color` | As --manual, with deleted text also red and new text also blue. |
| `--range <REFS>` | Look only in these blocks, such as p40..p60, or p40 for p40 to the end of its story. |
| `--clause <CLAUSE>` | Look only in this clause and the blocks under it, as view --clause names it, such as 11.1 or "Article III". Repeat it to look in several clauses. |
| `-o, --output <DOCX>` | With a .docx, the .docx to write, instead of NAME.edited.docx beside the input. It must not be the input: --in-place writes back to it. |
| `--in-place` | With a .docx, write the result back to it, atomically, and only if no one changed it meanwhile. A working copy needs no flag: the edit always writes it. |
| `--dry-run` | Show the receipt, and write nothing. A .docx needs neither -o nor --in-place for it. |
| `--quotes <QUOTES>` | Follow the paragraph's quotes (match), or keep them as typed. One of `match`, `as-typed`. Default `match`. |
| `--ignore-case` | Match letters whatever their case, so "net 30" also finds "Net 30". Without it the case must be the document's. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

`replace` changes text in one step. It reads the document, finds each OLD in the text as the final view reads it, and
applies every pair as tracked changes in one edit, all or nothing. There is no base to copy: stetdoc finds and edits in
the same run, so nothing can change between the two. OLD is matched across formatting, with curly and straight quotes
the same, and it must be in the document once. A .docx is written to NAME.edited.docx beside it, and the input is left
as it was, unless `-o FILE` names the output or `--in-place` writes back to the input. When NAME.edited.docx exists, the
edit goes to NAME.edited-2.docx, and so on, and the receipt says so.

```console
$ stetdoc replace contract.docx "thirty days" "sixty days" --author "Jane Doe"
replace "thirty days" with "sixty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 745747bfd39463c1:0:745747bfd39463c1
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.
wrote contract.edited.docx
contract.docx is unchanged. Run the next edit on contract.edited.docx to build on this one, or pass -o FILE or --in-place to choose.

```

Text that is there more than once, or not at all, is refused, and nothing is written. A refusal lists every place, with
its ref, its clause when it is in one, and the text around it, so the next try can use a longer OLD or a scope.

```console
$ stetdoc replace contract.docx "Supplier" "Vendor" -o edited.docx --author "Jane Doe"
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied, so retry at once, with no dry run: to change every one, pass --all; to pick one place, pass --range with its ref, such as --range p1..p1, or make OLD longer so it matches once.

$ stetdoc replace contract.docx "ninety days" "sixty days" -o edited.docx --author "Jane Doe"
? 3
error[not_found]: "ninety days" is not in the document as it reads now.
next: Nothing was applied. Write OLD exactly as the document reads now (stetdoc view FILE --find WORD shows it), or widen --range or --clause.

```

OLD is matched with its case. When it is there only in another case, the refusal says where, and `--ignore-case`
matches letters whatever their case.

```console
$ stetdoc replace contract.docx "payment is due" "Payment falls due" -o edited.docx --author "Jane Doe"
? 3
error[not_found]: "payment is due" is not in the document as it reads now.
Ignoring case, it is there once:
  p4 [Payment is due] within thirty days.
next: Nothing was applied. Run it again with --ignore-case to match the places listed, or write OLD in the document's case.

$ stetdoc replace contract.docx "payment is due" "Payment falls due" --ignore-case -o edited.docx --author "Jane Doe"
replace "payment is due" with "Payment falls due": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 8e37edb10d183515:0:8e37edb10d183515
0 replace p4: deletion #0, insertion #1
p4 Payment {--is--}{#0 JD}{++falls++}{#1 JD} due within thirty days.
validated 1 unit: no problems, and this file needs no separate check.
wrote edited.docx

```

`--all` replaces every occurrence and says how many. `--expect N` does the same only when there are exactly N, and
refuses with `count_mismatch` otherwise. `--range p40..p60` or `--clause 11.1` (a clause as `view --clause` names it)
limits where OLD is looked for.

```console
$ stetdoc replace contract.docx "Supplier" "Vendor" --all -o edited.docx --author "Jane Doe"
replace "Supplier" with "Vendor": 2 times, in 2 blocks: p1, p2
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base f2018f511cded939:0:f2018f511cded939
0 replace p1: deletion #0, insertion #1
1 replace p2: deletion #2, insertion #3
p1 The {--Supplier--}{#0 JD}{++Vendor++}{#1 JD} shall indemnify the Customer, and shall pay on demand.
p2 “Losses” means any loss, and the {--Supplier--}{#2 JD}{++Vendor++}{#3 JD}’s liability.
validated 2 units: no problems, and this file needs no separate check.
wrote edited.docx

$ stetdoc replace contract.docx "Supplier" "Vendor" --expect 3 -o edited.docx --author "Jane Doe"
? 3
error[count_mismatch]: "Supplier" is in the document 2 times, not 3:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied. Check the places listed, then pass the count they show with --expect, or limit the search with --range or --clause.

$ stetdoc replace contract.docx "Supplier" "Vendor" --range p2..p5 --dry-run --author "Jane Doe"
replace "Supplier" with "Vendor": once, at p2
dry run: nothing was applied. A new block has no ref until the edit is applied. 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:0:5737b772923cf8d3
0 replace p2: deletion #0, insertion #1
p2 “Losses” means any loss, and the {--Supplier--}{#0 JD}{++Vendor++}{#1 JD}’s liability.
validated 1 unit: no problems, and this file needs no separate check.

```

Several pairs go in one call and one edit. An empty NEW deletes OLD. `--dry-run` shows the receipt and writes nothing,
and a .docx needs neither `-o` nor `--in-place` for it. `--in-place` writes the .docx back to itself, as `edit` does,
and a working copy is written by the replace itself.

```console
$ stetdoc replace contract.docx "thirty days" "sixty days" ", and shall pay on demand" "" --in-place --author "Jane Doe"
replace "thirty days" with "sixty days": once, at p4
delete ", and shall pay on demand": once, at p1
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base 2fcb6ddbee598bc0:0:2fcb6ddbee598bc0
0 replace p4: deletion #0, insertion #1
1 delete p1: deletion #2
p1 The Supplier shall indemnify the Customer{--, and shall pay on demand--}{#2 JD}.
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 2 units: no problems, and this file needs no separate check.
wrote contract.docx

```

## `stetdoc comment`

Comment on text in one step: the comment is anchored to ANCHOR, found once in the document as it reads now.

```text
stetdoc comment [OPTIONS] <INPUT> <ANCHOR TEXT> <ANCHOR TEXT>...
```

| Argument | Meaning |
|---|---|
| `<INPUT>` | A working copy, or a .docx. A .docx is written to NAME.edited.docx beside it, unless -o or --in-place says where. |
| `<ANCHOR TEXT>` | ANCHOR and TEXT, as many pairs as you like, added together or not at all. ANCHOR is the text to anchor the comment to, matched as replace matches OLD; it must be there once (see --all and --expect). TEXT is the comment, and a newline in it is a paragraph break of the comment. |
| `--all` | Comment on every occurrence of the anchor, one comment each, and say how many. |
| `--expect <N>` | Comment on every occurrence of the anchor, and only if there are exactly N. |
| `--range <REFS>` | Look only in these blocks, such as p40..p60, or p40 for p40 to the end of its story. |
| `--clause <CLAUSE>` | Look only in this clause and the blocks under it, as view --clause names it, such as 11.1 or "Article III". Repeat it to look in several clauses. |
| `-o, --output <DOCX>` | With a .docx, the .docx to write, instead of NAME.edited.docx beside the input. It must not be the input: --in-place writes back to it. |
| `--in-place` | With a .docx, write the result back to it, atomically, and only if no one changed it meanwhile. A working copy needs no flag: the edit always writes it. |
| `--dry-run` | Show the receipt, and write nothing. A .docx needs neither -o nor --in-place for it. |
| `--quotes <QUOTES>` | Follow the paragraph's quotes (match), or keep them as typed. One of `match`, `as-typed`. Default `match`. |
| `--ignore-case` | Match letters whatever their case, so "net 30" also finds "Net 30". Without it the case must be the document's. |
| `--author <NAME>` | The author of the revisions, as Word shows it. Defaults to STETDOC_AUTHOR, then the name saved with stetdoc init. There is no other default. |
| `--date <DATE>` | The date of the revisions, YYYY-MM-DDTHH:MM:SS. Defaults to STETDOC_DATE, then the local clock. |

### Examples

`comment` adds a comment in one step: it finds ANCHOR in the text as the final view reads it, as `replace` finds OLD,
and anchors the comment to it. ANCHOR must be there once, and `--range` or `--clause` limits where it is looked for.
`--all` puts the same comment on every place, `--expect N` does so only when there are exactly N, and `--ignore-case`
matches letters whatever their case, as for `replace`. Without `-o` or `--in-place`, a .docx is written to
NAME.edited.docx beside it.

```console
$ stetdoc comment contract.docx "thirty days" "Can we agree sixty?" -o commented.docx --author "Jane Doe"
comment on "thirty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base a2a6bbc2e3a46398:0:a2a6bbc2e3a46398
0 comment p4: c0 added
p4 Payment is due within {c0}thirty days{/c0}.
validated 1 unit: no problems, and this file needs no separate check.
wrote commented.docx

$ stetdoc comment contract.docx "Supplier" "Which one?" -o commented.docx --author "Jane Doe"
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied, so retry at once, with no dry run: to comment on every one, pass --all; to pick one place, pass --range with its ref, such as --range p1..p1, or make the anchor longer so it matches once.

$ stetdoc comment contract.docx "Supplier" "Which one?" --all -o commented.docx --author "Jane Doe"
comment on "Supplier": 2 times, in 2 blocks: p1, p2
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base cfe4d86f3f81d849:0:cfe4d86f3f81d849
0 comment p1: c0 added
1 comment p2: c1 added
p1 The {c0}Supplier{/c0} shall indemnify the Customer, and shall pay on demand.
p2 “Losses” means any loss, and the {c1}Supplier{/c1}’s liability.
validated 2 units: no problems, and this file needs no separate check.
wrote commented.docx

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
compared: 3 revisions, tracked as Jane Doe 2026-10-06T09:30:00Z. base 8785d09e0fdbb2b7:0:8785d09e0fdbb2b7
Jane Doe: insertion 2, deletion 1
validated 2 units: no problems, and this file needs no separate check.
wrote redline.docx
next: stetdoc view redline.docx --revisions

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
| `--all` | List every finding the report keeps (the first 200), not only the first 20. |
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
note: Claude Code can also block other ways into a .docx, such as unzip or python-docx, with the guard hook the stetdoc plugin carries. To add it to this install, see https://github.com/clauseo-ai/stetdoc/blob/main/docs/agents.md#the-claude-code-guard-hook
note: no author is saved, and every revision needs one. Run stetdoc init --author "Your Name", or set STETDOC_AUTHOR, before you edit.
wrote .agents/skills/stetdoc/SKILL.md (Codex)
wrote .claude/skills/stetdoc/SKILL.md (Claude Code)
wrote AGENTS.md

```

## `stetdoc capabilities`

List the operations, whether each is tracked, the refusal codes and the format version.

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
stetdoc 0.9.0
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
  count_mismatch       exit 3
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
| 3 | target | `not_found`, `ambiguous_match`, `count_mismatch` |
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
