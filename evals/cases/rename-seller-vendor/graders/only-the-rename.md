---
type: regex
pattern: '^#[\d,]+ (insert|delete) [A-Z]+ \S+ p\d+ \"(?!Seller|Vendor)'
flags: m
match: not_contains
target: { source: file, path: .stetdoc-eval-report.txt }
---

Every revision deletes Seller or inserts Vendor; nothing else changed.
