---
type: regex
pattern: '^remaining-revisions: ([2-9]\d|\d{3,})$'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

The redline carries the differences: at least 20 tracked changes.
