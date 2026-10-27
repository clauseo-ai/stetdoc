`compare` writes the redline of two documents: the revised one, with what changed from the original as tracked changes.

```console
$ stetdoc compare contract.docx revised.docx -o redline.docx --author "Jane Doe"
compared: 3 revisions, tracked as Jane Doe 2026-10-06T09:30:00Z. base 8785d09e0fdbb2b7:0:8785d09e0fdbb2b7
Jane Doe: insertion 2, deletion 1
checked 2 units.
wrote redline.docx
next: stetdoc view redline.docx --revisions

```
