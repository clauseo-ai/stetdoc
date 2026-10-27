---
type: regex
pattern: '^  \{c\d+ ER [\d-]+\} [^\n]*(2x|2×|two times|twice|double)[^\n]*annual fee'
flags: mi
target: { source: file, path: .stetdoc-eval-report.txt }
---

The comment is by ER and asks for 2x annual fees.
