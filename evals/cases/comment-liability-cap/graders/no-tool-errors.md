---
type: regex
pattern: 'is_error\\?\"?:\s*true'
target: trace
match: not_contains
weight: 0.5
arm: both
---

No tool call failed. Half weight: a recovered error costs time, not correctness.
