# Quickstart

Install stetdoc, then make a tracked change and a comment in a real Word file. It takes about five minutes.

## Install stetdoc

```sh
brew install clauseo-ai/tap/stetdoc
```

On a machine without Homebrew, run `uv tool install stetdoc`. [Install](install.md) lists every other channel, Windows
included. Check that the binary is on your PATH:

```console
$ stetdoc --version
stetdoc 0.9.0

```

## Save your name

Every tracked change carries an author, and stetdoc never makes one up. Save yours once:

```sh
stetdoc init --author "Jane Doe"
```

`init` saves the name to `stetdoc/config.toml` in your config folder and installs the skill for the agents it finds on
this machine. The examples below sign as Jane Doe, so put your own name in their place.

## Get the sample contract

```sh
curl -fsSLO https://stetdoc.clauseo.chat/samples/contract.docx
```

It is a short contract with ten paragraphs and a table. Any `.docx` of your own works the same way.

## Find text

```console
$ stetdoc view contract.docx --find thirty
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{position: p4..p4 of find "thirty": 1 match in 1 block (1 of 1 blocks)}

```

Each hit starts with its ref. `p4` is the fourth paragraph, and it keeps that name for as long as you work on this
document. In a contract with numbered clauses, each hit also shows its clause, such as `{§11.1}`.

## Replace it

```console
$ stetdoc replace contract.docx 'thirty days' 'sixty days'
replace "thirty days" with "sixty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 745747bfd39463c1:0:745747bfd39463c1
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.
wrote contract.edited.docx
contract.docx is unchanged. Run the next edit on contract.edited.docx to build on this one, or pass -o FILE or --in-place to choose.

```

stetdoc wrote a new file, `contract.edited.docx`, and left `contract.docx` alone. The receipt shows the change as a
redline. `{--thirty--}` is the deleted word and `{++sixty++}` the inserted one. `{#0 JD}` names tracked change 0 and its
author's initials. Your run shows your own clock in the date, and so a different base token, because the base
records the state of the document, revisions included.

Only "thirty" became a revision. stetdoc compares the old and new text word by word, so "days" stays as it was.

## Comment on it

```console
$ stetdoc comment contract.edited.docx 'English law' 'Should this be Delaware law?' --in-place
comment on "English law": once, at p5
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 8a58a673b7838481:0:8a58a673b7838481
0 comment p5: c2 added
p5 This Agreement is governed by {c2}English law{/c2}.
validated 1 unit: no problems, and this file needs no separate check.
wrote contract.edited.docx

```

`--in-place` writes back to the file it read. Use it for every edit after the first, so each one builds on the last.

## Watch it refuse

Ask for a change that could land in more than one place:

```console
$ stetdoc replace contract.edited.docx Supplier Vendor
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied, so retry at once, with no dry run: to change every one, pass --all; to pick one place, pass --range with its ref, such as --range p1..p1, or make OLD longer so it matches once.

```

stetdoc wrote nothing. It listed both places and said what to do next. Your agent reads the same `next:` line. Here,
both should change:

```console
$ stetdoc replace contract.edited.docx Supplier Vendor --all --in-place
replace "Supplier" with "Vendor": 2 times, in 2 blocks: p1, p2
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base bf82df9a145d457f:0:bf82df9a145d457f
0 replace p1: deletion #3, insertion #4
1 replace p2: deletion #5, insertion #6
p1 The {--Supplier--}{#3 JD}{++Vendor++}{#4 JD} shall indemnify the Customer, and shall pay on demand.
p2 “Losses” means any loss, and the {--Supplier--}{#5 JD}{++Vendor++}{#6 JD}’s liability.
validated 2 units: no problems, and this file needs no separate check.
wrote contract.edited.docx

```

## See what you changed

```console
$ stetdoc view contract.edited.docx --revisions
{stet 1 view=ledger base=bf82df9a145d457f:0:bf82df9a145d457f mode=redline common-style="Normal" range=change:3..change:1}
{authors: JD "Jane Doe"}
{summary: 6 tracked changes (JD 6) · 1 comment · 1 section}
{changes: 6 changes}
#3 delete JD 2026-10-06 p1 "Supplier"
#4 insert JD 2026-10-06 p1 "Vendor"
#5 delete JD 2026-10-06 p2 "Supplier"
#6 insert JD 2026-10-06 p2 "Vendor"
#0 delete JD 2026-10-06 p4 "thirty"
#1 insert JD 2026-10-06 p4 "sixty"
{position: change:3..change:1 of changes (6 of 6 changes)}

```

Now open `contract.edited.docx` in Word. Each change is a tracked change under your name, and the comment sits in the
margin beside "English law". Accept or reject them there like any other revision. Only the edited elements changed, and
the rest of the file is byte for byte what `contract.docx` held.

## Next

- [Use stetdoc with your agent](agents.md) sets up Claude Code, Codex, Cursor and the rest. Then ask your agent to make
  these edits for you.
- [How stetdoc works](concepts.md) covers the working copy for longer edits, refs and base tokens, and why stetdoc
  refuses what it refuses.
- The [CLI reference](reference/cli.md) lists every command and flag.
