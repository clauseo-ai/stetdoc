---
name: stetdoc
description: Read, redline, comment on and compare Word documents (.docx) with real tracked changes. Use whenever a task involves reading or editing a .docx file.
---

# stetdoc

The `stetdoc` command reads and edits .docx files. Every edit becomes a Word tracked change with an author, and
everything you do not touch stays byte-identical.

Its instructions are embedded in the binary, so they always match the installed version. Before you work on a .docx, run

```
stetdoc skills get core
```

and follow what it prints. `stetdoc skills get operations` and `stetdoc skills get errors` are its references.

If `stetdoc` is not found, tell the user it is not installed, and do not edit the .docx with other tools.
