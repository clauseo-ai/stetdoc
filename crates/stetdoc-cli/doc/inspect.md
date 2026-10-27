```console
$ stetdoc inspect contract.docx p1
block: p1 is a paragraph in the body story
characters 0..67: plain
tracked changes: none in this block: all of its text is the document's own, even text that looks like a revision
comments: none on this block

```

A block lists its tracked changes and comments, or says it has none, so text that only looks like a change can be told
from one.

```console
$ stetdoc inspect tracked.docx p2
block: p2 is a paragraph in the body story
characters 0..10: plain
change #2: delete by Eric White on 2026-01-03, deletes "gross"
comments: none on this block

```
