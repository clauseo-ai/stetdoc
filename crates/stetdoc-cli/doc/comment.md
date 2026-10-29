`comment` adds a comment in one step: it finds ANCHOR in the text as the final view reads it, as `replace` finds OLD,
and anchors the comment to it. ANCHOR must be there once, and `--range` or `--clause` limits where it is looked for.
`--all` puts the same comment on every place, `--expect N` does so only when there are exactly N, and `--ignore-case`
matches letters whatever their case, as for `replace`. Without `-o` or `--in-place`, a .docx is written to
NAME.edited.docx beside it.

```console
$ stetdoc comment contract.docx "thirty days" "Can we agree sixty?" -o commented.docx --author "Jane Doe"
comment on "thirty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base a2a6bbc2e3a46398:0:a2a6bbc2e3a46398
0 comment p4: c0 added
p4 Payment is due within {c0}thirty days{/c0}.
validated 1 unit: no problems, and this file needs no separate check.
wrote commented.docx

$ stetdoc comment contract.docx "Supplier" "Which one?" -o commented.docx --author "Jane Doe"
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied, so retry at once, with no dry run: to comment on every one, pass --all; to pick one place, pass --range with its ref, such as --range p1..p1, or make the anchor longer so it matches once.

$ stetdoc comment contract.docx "Supplier" "Which one?" --all -o commented.docx --author "Jane Doe"
comment on "Supplier": 2 times, in 2 blocks: p1, p2
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base cfe4d86f3f81d849:0:cfe4d86f3f81d849
0 comment p1: c0 added
1 comment p2: c1 added
p1 The {c0}Supplier{/c0} shall indemnify the Customer, and shall pay on demand.
p2 “Losses” means any loss, and the {c1}Supplier{/c1}’s liability.
validated 2 units: no problems, and this file needs no separate check.
wrote commented.docx

```
