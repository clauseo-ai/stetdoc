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
