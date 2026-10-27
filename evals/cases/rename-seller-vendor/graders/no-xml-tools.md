---
type: tool_used
tool: Bash
input_match: '\b(unzip|zipinfo|bsdtar|7z|ditto|python3?|perl|ruby|node|xmllint|pandoc|soffice|libreoffice|docx2python)\b|document\.xml|word/'
min: 0
max: 0
arm: both
---

The agent never opens the .docx another way: no unzip, no scripting language, no XML.
