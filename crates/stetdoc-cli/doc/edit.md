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
