---
type: regex
pattern: '^#[\d,]+ insert ER \S+ p27[2-4] \"[^\n]*New York'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

Clause 23.1 (p273): New York is a tracked insertion by ER.
