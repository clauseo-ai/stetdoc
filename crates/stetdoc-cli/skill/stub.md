---
name: stetdoc
description: Read, redline, comment on and compare Word documents (.docx) with real tracked changes. Use whenever a task involves reading or editing a .docx file.
metadata:
  stetdoc-version: "{{version}}"
---

# stetdoc

stetdoc edits .docx files as Word tracked changes.

In Claude Code, use the `stetdoc` command. Elsewhere, with stetdoc's MCP tools, call `skills_get` first and follow it.
Everyday work needs only these:

```
stetdoc view a.docx --find 'net 30' --find 'governing law'
stetdoc replace a.docx 'net 30' 'net 45' 'thirty' 'sixty' -o a.redline.docx
stetdoc comment a.redline.docx 'Liability Cap' 'Mutual?' --in-place
stetdoc compare old.docx new.docx -o redline.docx
```

Each OLD must occur once: if several places are listed, use a longer OLD or `--clause 11.1`. `--all` replaces every
one; an empty NEW deletes. Keep the user's original unchanged unless asked: write with `-o`, and use `--in-place` only
on a file this task wrote or the user asked to update. For anything else (adding paragraphs, accepting or rejecting
revisions, formatting), run `stetdoc skills get core`.

If `stetdoc` is not found, tell the user it is not installed, and stop. Never read or edit a .docx another way (unzip,
XML, python-docx).
