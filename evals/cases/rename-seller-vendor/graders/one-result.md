---
type: regex
pattern: '^== '
flags: m
match: "count:1"
target: { source: file, path: .stetdoc-eval-report.txt }
---

Exactly one .docx was written or changed (a new file, or the original updated in place).
