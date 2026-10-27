`comment` adds a comment in one step: it finds ANCHOR in the text as the final view reads it, as `replace` finds OLD,
and anchors the comment to it. ANCHOR must be there once, and `--range` or `--clause` limits where it is looked for.

```console
$ stetdoc comment contract.docx "thirty days" "Can we agree sixty?" -o commented.docx --author "Jane Doe"
comment on "thirty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base a2a6bbc2e3a46398:0:a2a6bbc2e3a46398
0 comment p4: c0 added
p4 Payment is due within {c0}thirty days{/c0}.
checked 1 unit.
wrote commented.docx

$ stetdoc comment contract.docx "Supplier" "Which one?" -o commented.docx --author "Jane Doe"
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied. Make the anchor longer so it matches once, or limit the search with --range or --clause.

```
