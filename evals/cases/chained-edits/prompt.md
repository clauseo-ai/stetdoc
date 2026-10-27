---
description: "Three edits chained into the same redline, the last rejecting the first."
tags: [edit, chain]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

In docs/master-supply-agreement.docx, change the payment terms to net 45 days and save the redline as a new file. Then, in that redline, also change the governing law to England and Wales, and then reject your own payment-term change again (ids as listed). Do these as three separate edits.
