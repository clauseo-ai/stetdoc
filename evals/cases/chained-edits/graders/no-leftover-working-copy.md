---
type: regex
pattern: '^leftover-working-copy:'
flags: m
weight: 0.5
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

No NAME.stet/ working copy is left next to the documents.
