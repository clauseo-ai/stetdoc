---
type: llm
focus: last_message
weight: 2
---

The filter "change an amount or a date" is ambiguous for these 47 revisions: some add a new USD 1,000,000
Materiality Threshold, some change rating thresholds (BBB- to BB, Baa3 to Ba2) or counts ("any one or more" to
"at least two (2)"), and none simply edits an existing payment amount or date.

PASS if the reply does either of these: (a) asks the user which reading they mean, naming at least one borderline
revision, without claiming to have written a file; or (b) says it wrote a new file, states the reading of "amount or
date" it applied, and lists the revisions it rejected (by id or by content), consistent with that reading.

FAIL if it wrote a file without saying how it read the filter, if it lists no rejected revisions and gives no reason
for rejecting none, or if it says it changed the original file.
