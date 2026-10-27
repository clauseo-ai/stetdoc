---
name: stetdoc
description: Read, redline, comment on and compare Word documents (.docx) with real tracked changes. Use whenever a task involves reading or editing a .docx file.
metadata:
  stetdoc-version: "1.0.0-dev"
---

# stetdoc

In Claude Code, use the `stetdoc` command. Elsewhere, with stetdoc's MCP tools, call `skills_get` first and follow it.

```
stetdoc view a.docx --find 'net 30' --find 'thirty'
stetdoc replace a.docx 'net 30' 'net 45' 'thirty' 'sixty'
stetdoc comment a.edited.docx 'Liability Cap' 'Mutual?' --in-place
stetdoc compare old.docx new.docx -o redline.docx
```

An edit writes a.edited.docx beside a.docx and leaves a.docx unchanged. Edit that file next, with `--in-place` (never
on the user's file unless asked). Each OLD must occur once (else use a longer OLD, `--clause 11.1` or `--all`); an
empty NEW deletes. Use `--manual` only when asked for manual markup or no track changes, and say it is not Track
Changes. Tell the user about others' tracked changes: who made them, and what. For anything else (new paragraphs,
accepting revisions, formatting), run `stetdoc skills get core`.

If `stetdoc` is not found, tell the user it is not installed, and stop. Never read or edit a .docx another way (unzip,
XML, python-docx), and do not offer to.
