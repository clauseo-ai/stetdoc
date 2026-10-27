---
type: regex
pattern: '^created: .*\.docx$'
flags: m
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

A question about the document writes no .docx.
