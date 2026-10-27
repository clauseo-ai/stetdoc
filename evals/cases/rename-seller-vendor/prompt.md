---
description: "Rename a defined party throughout (89 occurrences) as tracked changes."
tags: [edit, bulk]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

Change 'Seller' to 'Vendor' throughout docs/master-supply-agreement.docx.
