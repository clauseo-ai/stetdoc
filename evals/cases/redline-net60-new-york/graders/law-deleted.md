---
type: regex
pattern: '^#[\d,]+ delete ER \S+ p273 \"[^\n]*Singapore'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

Clause 23.1 (p273): Singapore is a tracked deletion by ER.
