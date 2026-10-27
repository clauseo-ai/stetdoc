---
type: regex
pattern: '^(?![\s\S]*\n== )|Co-Calculation Agent'
target: { source: file, path: .stetdoc-eval-report.txt }
---

If the agent wrote a file, the text-only changes are accepted (the Co-Calculation Agent wording of revision #3 is in it). Passes when no file was written, since asking first is allowed.
