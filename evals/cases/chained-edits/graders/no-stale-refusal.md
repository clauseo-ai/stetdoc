---
type: regex
pattern: 'error\[stale_ref\]'
target: trace
match: not_contains
arm: both
---

Each edit started from the previous receipt's base: stetdoc never refused one as stale.
