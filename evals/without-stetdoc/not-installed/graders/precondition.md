---
type: regex
pattern: '^stetdoc-on-path: no$'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

Precondition, not agent behaviour: stetdoc really was missing from PATH. Run this case through evals/run.sh.
