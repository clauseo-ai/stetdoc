---
type: regex
pattern: '11\.1 Payment for each Shipment[^\n]*?\b(sixty|60)\b[^\n]*?days[\s\S]*23\.1 This Agreement[^\n]*New York'
target: { source: file, path: .stetdoc-eval-report.txt }
---

Read as final, clause 11.1 says sixty days and clause 23.1 names New York law.
