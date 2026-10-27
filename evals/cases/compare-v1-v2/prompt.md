---
description: "Compare two versions into a redline."
tags: [compare]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

Make me a redline comparing docs/isda-confirmation-v1.docx (original) against docs/isda-confirmation-v2.docx (revised).
