# How stetdoc works

Every stetdoc command relies on four ideas: the working copy, refs, base tokens and receipts. Each is explained here,
with the reason stetdoc refuses an edit rather than guess. The [glossary](../GLOSSARY.md) defines each term exactly.

## The document as text

A `.docx` is a zip of XML. Word splits one sentence across many runs, stores tracked changes as XML structure, and
refuses a file whose elements fall out of schema order. stetdoc keeps all of that to itself and shows your agent the
document as lines of text, one per paragraph:

```console
$ stetdoc view contract.docx --find thirty
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{position: p4..p4 of find "thirty": 1 match in 1 block (1 of 1 blocks)}

```

The first line names the view, the base token and the mode. `redline` shows tracked changes marked. `final` shows the
text with every change accepted, and `original` with every change rejected. Text in braces is markup that stetdoc adds.
Everything else is the document's own text.

## Refs

Each block has a short ref. `p4` is a paragraph, `t1` a table and `r2` a table row. stetdoc numbers the blocks in order
when it opens a document, and a ref stays with its block for as long as you work on that document. Deleting a paragraph
never renumbers the others, and a paragraph an edit adds gets a new ref when the edit applies. stetdoc never writes refs
into the `.docx`.

Opening the saved file starts the numbering again. That is why each edit of a `.docx` names the file it wrote and tells
you to make the next edit on that file, and why a long task uses a working copy.

## The base token

Every view and receipt carries a base token, such as `5737b772923cf8d3:1:6a07baa2d488eeb5`. It names one state of
one document by the document, its count of edits and a hash of their history. A batch of operations names the
base it was written against, so stetdoc can tell when the document moved on.

stetdoc refuses a batch that names a block changed since its base, because the agent wrote it from text that is no
longer there:

```console
$ stetdoc edit contract.stet <<'EOF'
base: 5737b772923cf8d3:0:5737b772923cf8d3
---
@replace
ref: p4
find: sixty
with: ninety
EOF
? 4
error[stale_ref]: p4 was changed in version 1, after version 0, which this batch was written for. View it again. (operation 0)
next: View the block again, then rewrite the operation from what it shows.

```

A batch from an older state that touches only blocks unchanged since then still applies. stetdoc refuses outright a
batch from another document, or from a history this document doesn't have.

## The working copy

For a long task, unpack the document once into a working copy and keep it until the task is done:

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ ls -A contract.stet
.stet
document.md

```

The readable files are plain text that any tool can edit. `document.md` holds the body, `notes.md` the footnotes and
endnotes, `headers.md` the headers and footers, and `comments.md` the comments, which are read-only. stetdoc writes only
the files the document needs, so this contract gets `document.md` alone. Each line starts with a ref:

```console
$ cat contract.stet/document.md
{stet 1 file=document.md base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal"}
{rules: edit the text after a ref · markers in braces and {§"numbers"} are read-only · a line with no ref is a new paragraph, text only · delete a line to delete its paragraph · blank lines and trailing spaces are ignored · write \{ \} \\ for a literal brace or backslash, \t for a tab}
{summary: 1 section}
p1 The Supplier shall indemnify the Customer, and shall pay on demand.
p2 “Losses” means any loss, and the Supplier’s liability.
p3 Emoji 😀 then a tab\tand the end.
p4 Payment is due within thirty days.
p5 This Agreement is governed by English law.
t1 {table cols=2}
r1 {row}
p6 a1
p7 b1
r2 {row}
p8 a2
p9 b2
{/table}
p10 After the table.

```

The complement, `.stet/`, holds the original bytes, the refs, the base and the hashes of the readable files as
stetdoc wrote them, which is everything it needs to rebuild the exact package. Never edit it. stetdoc verifies it rather than trusting
it, and refuses a complement that doesn't match with `corrupt_complement`.

Edit a readable file with any tool, then `check` shows the redline that `pack` will write:

```console
$ perl -pi -e 's/thirty days/sixty days/' contract.stet/document.md

$ stetdoc check contract.stet
Pending edits, as pack would write them:
dry run: nothing was applied. A new block has no ref until the edit is applied. 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:0:5737b772923cf8d3
0 replace p4 (document.md line 7): deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.

Validation: no violations in 1 part.

$ stetdoc pack contract.stet
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:1:6a07baa2d488eeb5
0 replace p4 (document.md line 7): deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.
wrote contract.edited.docx
working copy contract.stet is at base 5737b772923cf8d3:1:6a07baa2d488eeb5. It stays until you delete the folder, or pack again with --done to remove it.

```

stetdoc compares the edited line with the one it wrote, word by word, and turns the difference into insertions and
deletions. After `pack`, the working copy is at the new base, so the agent carries on editing with the same refs. Pass
`--done` on the last pack to remove it.

Every write to a working copy survives a crash, and a second process waits for the first to finish.

## Receipts and the redline notation

Every edit prints a receipt. It says what stetdoc applied, under whose name, the new base, each change as a redline, whether
the result validated, and the file it wrote. Views use the same notation:

| Notation | Meaning |
| --- | --- |
| `{--thirty--}` | Deleted text, as a tracked change |
| `{++sixty++}` | Inserted text, as a tracked change |
| `{#0 JD}` | The tracked change before it, with its id and its author's initials. `--revisions` lists it by that id |
| `{authors: JD "Jane Doe"}` | The key to the initials |
| `{c2}English law{/c2}` | The text that comment `c2` is on |
| `{§11.1}` | The clause number Word shows for the paragraph |
| `{~ "sixty" 1}` | A margin note about the line, such as how many times a search matched it. It isn't document text |

Text such as `[DELETED: thirty]` with no braces and no id is literal text that someone typed into the document. A real
tracked change always has its id.

## Tracked and manual markup

Every edit is a tracked change unless you ask otherwise. Some readers want a manual redline instead, with the old text
struck through and the new text underlined, and no Track Changes. `--manual` writes that:

```console
$ stetdoc replace contract.docx 'thirty days' 'sixty days' --manual -o manual.docx
replace "thirty days" with "sixty days": once, at p4
applied 1 operation, as manual markup: old text kept and struck through, new text underlined, and no tracked changes. Each operation says what it struck and underlined; the text of a block below runs old and new together. Tell the reader this is a manual redline, not Track Changes. base f287dd535956563b:0:f287dd535956563b
0 replace p4: struck "thirty", underlined "sixty"
p4 Payment is due within thirtysixty days.
validated 1 unit: no problems, and this file needs no separate check.
wrote manual.docx

```

Manual markup is plain formatting, so nobody can accept or reject it in Word.

## Validation and refusals

stetdoc checks every edit against the OOXML schema and against Word's own rules before it writes a byte. An edit may
not add a problem the document didn't already have. Problems that were in the input never block an edit, and a receipt
sums them up in one line.

A batch applies completely or not at all. When stetdoc can't do exactly what you asked, it writes nothing and refuses
with a stable code, a message and a `next:` line that says what to do. Text that matches twice is `ambiguous_match`, a
ref from an older state is `stale_ref`, and an output file someone else wrote is `stale_target`. The exit status gives
the class of the failure, so a script can tell a bad request from a stale one. The [error reference](reference/errors.md)
lists every code.

## The round trip

Open a document and save it without editing, and you get the same bytes back. Edit one paragraph, and only that
paragraph's elements change. Styles, numbering, comments, other people's tracked changes and macros stay as their
authors left them. stetdoc tests this every night on about 48,000 documents. [`FIDELITY.md`](../FIDELITY.md) lists,
feature by feature, what stetdoc keeps exactly, what it normalizes, what it shows read-only and what it leaves out.

## Clause numbers

Lawyers cite clauses by number, such as 2.01(a) or 11.1. In many documents Word works those numbers out from list
definitions when it draws the page, so they appear nowhere in the file. stetdoc computes them the same way and shows
each one in braces. In a long credit agreement, the file holds no "2.01(a)" anywhere, yet a search finds it:

```console
$ stetdoc view credit-agreement.docx --find 'severally (and not jointly)'
{stet 1 view=find base=50433071aa4742bf:0:50433071aa4742bf mode=redline common-style="Body Text" range=p132..p136}
{summary: 1 section}
{in p130 "Article II — THE COMMITMENTS AND CREDIT EXTENSIONS"}
p132 {§2.01(a)} …set forth herein, each Lender severally (and not jointly) agrees to make a term loan (each…
p136 {§2.02(a)} …set forth herein, each Lender severally (and not jointly) agrees to make revolving loans…
{position: p132..p136 of find "severally (and not jointly)": 2 matches in 2 blocks (2 of 2 blocks)}

```

`--clause 2.01(a)` views a clause by its number, and `stetdoc lookup` finds what cites it. A number that Word computes
is read-only, and stetdoc refuses an edit that changes one with `readonly_region`.
