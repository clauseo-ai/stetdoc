---
type: regex
pattern: '^\{authors: ER \"Eval Reviewer\"\}$'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

Every revision in the output is by the configured author, Eval Reviewer (ER), and by no one else.
