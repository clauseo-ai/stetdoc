---
type: regex
pattern: '^created: .*\.docx$'
flags: m
match: "count:1"
target: { source: file, path: .stetdoc-eval-report.txt }
---

Exactly one new .docx was written.
