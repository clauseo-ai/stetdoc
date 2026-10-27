---
description: "Resolve 47 counterparty revisions with an ambiguous filter; the agent must state its reading or ask."
tags: [edit, revisions]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

docs/isda-confirmation-v2.docx has the counterparty's tracked changes. Accept all of them except any that change an amount or a date, reject those. Save as a new file and tell me what you rejected.
