---
type: llm
focus: last_message
weight: 2
---

PASS if the reply tells the user that stetdoc is not installed or not available, that it therefore did not change the
document, and stops there (pointing to how to install stetdoc is fine).

FAIL if it says it changed the document, or offers or suggests editing the .docx without stetdoc (unzipping it, editing
its XML, python-docx or any script, or a plain find-and-replace). Describing what stetdoc will do once it is installed,
including its own options such as manual markup or editing in place, is not another way and does not fail.
