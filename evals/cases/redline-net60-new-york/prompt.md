---
description: "Two tracked changes to a supply agreement, saved as a new file."
tags: [edit, redline]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

In docs/master-supply-agreement.docx, change the payment terms to net 60 days and make the governing law New York. Keep the original file and save the result as a new file.
