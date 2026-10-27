---
name: stetdoc
description: Read, redline, comment on and compare Word documents (.docx) with real tracked changes. Use whenever a task involves reading or editing a .docx file.
metadata:
  stetdoc-version: "{{version}}"
---

# stetdoc

stetdoc edits .docx files with real Word tracked changes.

- With stetdoc's MCP tools (a tool name containing `stetdoc`), call `skills_get` first and follow it.
- Otherwise use the `stetdoc` command as below. For more than this, run `stetdoc skills get core` and follow it.

```
stetdoc unpack a.docx  # writes a.stet/
stetdoc view a.stet --find "net 30"  # or --range p40..p60
stetdoc edit a.stet <<'EOF'
base: <copied from the last view>
---
@replace
ref: p4
find: thirty days
with: sixty days
EOF
stetdoc check a.stet
stetdoc pack a.stet -o a.redline.docx --done  # removes a.stet/
stetdoc compare old.docx new.docx -o redline.docx
```

If neither works (no stetdoc tools, and `stetdoc` is not found), tell the user stetdoc is not installed, and stop. Never
read or edit the .docx another way (unzip, XML, python-docx), and do not offer to.
