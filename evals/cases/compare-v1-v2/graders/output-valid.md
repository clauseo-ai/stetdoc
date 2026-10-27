---
type: regex
pattern: '^valid: yes$'
flags: m
target: { source: file, path: .stetdoc-eval-report.txt }
---

Every .docx the agent wrote passes stetdoc check with no violation the inputs did not already have.
