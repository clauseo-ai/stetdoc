`replace` changes text in one step. It reads the document, finds each OLD in the text as the final view reads it, and
applies every pair as tracked changes in one edit, all or nothing. There is no base to copy: stetdoc finds and edits in
the same run, so nothing can change between the two. OLD is matched across formatting, with curly and straight quotes
the same, and it must be in the document once.

```console
$ stetdoc replace contract.docx "thirty days" "sixty days" -o edited.docx --author "Jane Doe"
replace "thirty days" with "sixty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 745747bfd39463c1:0:745747bfd39463c1
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 1 unit.
wrote edited.docx

```

Text that is there more than once, or not at all, is refused, and nothing is written. A refusal lists every place, with
its ref, its clause when it is in one, and the text around it, so the next try can use a longer OLD or a scope.

```console
$ stetdoc replace contract.docx "Supplier" "Vendor" -o edited.docx --author "Jane Doe"
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied. Make OLD longer so it matches once, limit the search with --range or --clause, or pass --all to change every one.

$ stetdoc replace contract.docx "ninety days" "sixty days" -o edited.docx --author "Jane Doe"
? 3
error[not_found]: "ninety days" is not in the document as it reads now.
next: Nothing was applied. Write OLD exactly as the document reads now (stetdoc view FILE --find WORD shows it), or widen --range or --clause.

```

`--all` replaces every occurrence and says how many. `--expect N` does the same only when there are exactly N, and
refuses with `count_mismatch` otherwise. `--range p40..p60` or `--clause 11.1` (a clause as `view --clause` names it)
limits where OLD is looked for.

```console
$ stetdoc replace contract.docx "Supplier" "Vendor" --all -o edited.docx --author "Jane Doe"
replace "Supplier" with "Vendor": 2 times, in 2 blocks: p1, p2
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base f2018f511cded939:0:f2018f511cded939
0 replace p1: deletion #0, insertion #1
1 replace p2: deletion #2, insertion #3
p1 The {--Supplier--}{#0 JD}{++Vendor++}{#1 JD} shall indemnify the Customer, and shall pay on demand.
p2 “Losses” means any loss, and the {--Supplier--}{#2 JD}{++Vendor++}{#3 JD}’s liability.
checked 2 units.
wrote edited.docx

$ stetdoc replace contract.docx "Supplier" "Vendor" --expect 3 -o edited.docx --author "Jane Doe"
? 3
error[count_mismatch]: "Supplier" is in the document 2 times, not 3:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied. Check the places listed, then pass the count they show with --expect, or limit the search with --range or --clause.

$ stetdoc replace contract.docx "Supplier" "Vendor" --range p2..p5 --dry-run --author "Jane Doe"
replace "Supplier" with "Vendor": once, at p2
dry run: nothing was applied. A new block has no ref until the edit is applied. 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:0:5737b772923cf8d3
0 replace p2: deletion #0, insertion #1
p2 “Losses” means any loss, and the {--Supplier--}{#0 JD}{++Vendor++}{#1 JD}’s liability.
checked 1 unit.

```

Several pairs go in one call and one edit. An empty NEW deletes OLD. `--dry-run` shows the receipt and writes nothing,
and a .docx needs neither `-o` nor `--in-place` for it. `--in-place` writes the .docx back to itself, as `edit` does,
and a working copy is written by the replace itself.

```console
$ stetdoc replace contract.docx "thirty days" "sixty days" ", and shall pay on demand" "" --in-place --author "Jane Doe"
replace "thirty days" with "sixty days": once, at p4
delete ", and shall pay on demand": once, at p1
applied 2 operations, tracked as Jane Doe 2026-10-06T09:30:00Z. base 2fcb6ddbee598bc0:0:2fcb6ddbee598bc0
0 replace p4: deletion #0, insertion #1
1 delete p1: deletion #2
p1 The Supplier shall indemnify the Customer{--, and shall pay on demand--}{#2 JD}.
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 2 units.
wrote contract.docx

```
