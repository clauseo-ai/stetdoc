`compare` writes the redline of two documents: the revised one, with what changed from the original as tracked changes.

```console
$ stetdoc compare contract.docx revised.docx -o redline.docx --author "Jane Doe"
compared: 3 revisions, tracked as Jane Doe 2026-10-06T09:30:00Z. base 3953ae684513612a:1:b1378820043c0bb8
Jane Doe: insertion 2, deletion 1
checked 2 units.
wrote redline.docx

```
