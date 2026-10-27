---
type: regex
pattern: '^#[\d,]+ [a-z -]+? \S+ \d{4}-\d\d-\d\d p(?!(?:273) )\d+'
flags: m
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

No revision outside the paragraphs the task names (p273).
