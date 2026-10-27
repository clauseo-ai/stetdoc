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
