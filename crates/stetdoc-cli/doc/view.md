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
