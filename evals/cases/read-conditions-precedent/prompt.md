---
description: "Answer a question about a long credit agreement, citing sections; nothing is written."
tags: [read]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

In docs/credit-agreement.docx, what are the conditions precedent to the initial loan? Cite the section numbers.
