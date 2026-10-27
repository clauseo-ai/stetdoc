---
type: tool_used
tool: Bash
input_match: '\b(?:unzip|zipinfo|bsdtar|7z|ditto|xmllint|soffice|libreoffice|docx2python|pandoc)\b|document\.xml|\bword/|\[Content_Types\]\.xml|\b(?:python3?|perl|ruby|node)\b(?:(?!(?:\n|(?<!\\)(?:\\\\)*\\n)|;|&&|\|)[\s\S])*?(?:\s-[a-zA-Z]*c\s+(\\"|"|'')(?:(?!\1)[\s\S])*?\.docx\b|<<-?\s*(?:\\"|"|'')?(\w+)(?!\w)(?:(?!(?:\n|(?<!\\)(?:\\\\)*\\n)\2(?!\w))[\s\S])*?\.docx\b|\.docx\b)'
min: 0
max: 0
arm: both
---

The agent never opens the .docx another way: no unzip, no scripting language, no XML.

A script counts only when it touches a .docx (its heredoc or -c body included); a tool name, document.xml, word/ or [Content_Types].xml (an unzipped package) always counts. A script that edits the agent's own text files (an operations file, a stetdoc working copy's Markdown) does not.
