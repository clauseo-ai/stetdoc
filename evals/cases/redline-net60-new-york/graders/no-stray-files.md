---
type: regex
pattern: '^created: (?!.*\.docx$)'
flags: m
weight: 0.5
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

The agent created no file other than .docx files (no scripts, no temp files in the workspace).
