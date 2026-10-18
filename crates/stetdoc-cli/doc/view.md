With no flag, a view shows the whole body if it fits the budget, and its outline otherwise.

```console
$ stetdoc view contract.docx --find "thirty days" --context 0
{stet 1 view=find base=5737b772923cf8d3:0:5737b772923cf8d3 mode=redline common-style="Normal" range=p4..p4}
{summary: 1 section}
p4 Payment is due within thirty days.
{~ 1 match}
{position: p4..p4 of find "thirty days" (1 of 1 blocks)}

```
