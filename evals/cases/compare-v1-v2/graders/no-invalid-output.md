---
type: regex
pattern: '^valid: no$'
flags: m
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

No .docx the agent wrote fails stetdoc check.
