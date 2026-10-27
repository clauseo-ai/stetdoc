---
type: regex
pattern: '^\{threads: 1 thread\}$'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

The output has exactly one comment thread.
