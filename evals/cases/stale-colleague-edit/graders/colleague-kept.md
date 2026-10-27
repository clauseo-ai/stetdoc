---
type: regex
pattern: '^#[\d,]+ insert DC \S+ p146 \"[^\n]*rated at least A-'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

The colleague's change (p146, rated at least A-) is still there, under the colleague's name.
