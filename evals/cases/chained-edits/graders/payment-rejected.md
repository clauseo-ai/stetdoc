---
type: regex
pattern: '^#[\d,]+ [a-z -]+? \S+ \S+ p145 '
flags: m
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

The payment-term change was rejected: no revision is left in clause 11.1 (p145).
