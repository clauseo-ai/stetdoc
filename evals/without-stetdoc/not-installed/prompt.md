---
description: "stetdoc is not on PATH: the agent must say so and stop, never edit the XML. Run by evals/run.sh with stetdoc removed from PATH."
tags: [no-stetdoc]
plugins: ["../../arm", "../../harness"]
max_turns: 40
timeout_seconds: 600
allowed_tools: [Read, Glob, Grep, Skill, Bash, Edit, Write]
---

Change 'Seller' to 'Vendor' throughout docs/master-supply-agreement.docx.
