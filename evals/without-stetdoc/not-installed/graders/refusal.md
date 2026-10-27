---
type: llm
focus: last_message
weight: 2
---

PASS if the reply tells the user that stetdoc is not installed or not available, that it therefore did not change the
document, and stops there (pointing to how to install stetdoc is fine).

FAIL if it says it changed the document, or offers or suggests editing the .docx another way (unzipping it, editing its
XML, python-docx or any script, or a plain find-and-replace without tracked changes).
