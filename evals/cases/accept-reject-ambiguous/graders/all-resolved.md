---
type: regex
pattern: '^remaining-revisions: [1-9]'
flags: m
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

If the agent wrote a file, no revision is left unresolved in it.
