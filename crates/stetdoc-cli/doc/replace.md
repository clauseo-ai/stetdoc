`replace` changes text in one step. It reads the document, finds each OLD in the text as the final view reads it, and
applies every pair as tracked changes in one edit, all or nothing. There is no base to copy: stetdoc finds and edits in
the same run, so nothing can change between the two. OLD is matched across formatting, with curly and straight quotes
the same, and it must be in the document once. A .docx is written to NAME.edited.docx beside it, and the input is left
as it was, unless `-o FILE` names the output or `--in-place` writes back to the input. When NAME.edited.docx exists, the
edit goes to NAME.edited-2.docx, and so on, and the receipt says so.

```console
$ stetdoc replace contract.docx "thirty days" "sixty days" --author "Jane Doe"
replace "thirty days" with "sixty days": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 745747bfd39463c1:0:745747bfd39463c1
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
validated 1 unit: no problems, and this file needs no separate check.
wrote contract.edited.docx
contract.docx is unchanged. Run the next edit on contract.edited.docx to build on this one, or pass -o FILE or --in-place to choose.

```

Text that is there more than once, or not at all, is refused, and nothing is written. A refusal lists every place, with
its ref, its clause when it is in one, and the text around it, so the next try can use a longer OLD or a scope.

```console
$ stetdoc replace contract.docx "Supplier" "Vendor" -o edited.docx --author "Jane Doe"
? 3
error[ambiguous_match]: "Supplier" is in the document 2 times, and it must be there once:
  p1 The [Supplier] shall indemnify the Customer,…
  p2 …sses” means any loss, and the [Supplier]’s liability.
next: Nothing was applied, so retry at once, with no dry run: to change every one, pass --all; to pick one place, pass --range with its ref, such as --range p1..p1, or make OLD longer so it matches once.

$ stetdoc replace contract.docx "ninety days" "sixty days" -o edited.docx --author "Jane Doe"
? 3
error[not_found]: "ninety days" is not in the document as it reads now.
next: Nothing was applied. Write OLD exactly as the document reads now (stetdoc view FILE --find WORD shows it), or widen --range or --clause.

```

OLD is matched with its case. When it is there only in another case, the refusal says where, and `--ignore-case`
matches letters whatever their case.

```console
$ stetdoc replace contract.docx "payment is due" "Payment falls due" -o edited.docx --author "Jane Doe"
? 3
error[not_found]: "payment is due" is not in the document as it reads now.
Ignoring case, it is there once:
  p4 [Payment is due] within thirty days.
next: Nothing was applied. Run it again with --ignore-case to match the places listed, or write OLD in the document's case.

$ stetdoc replace contract.docx "payment is due" "Payment falls due" --ignore-case -o edited.docx --author "Jane Doe"
replace "payment is due" with "Payment falls due": once, at p4
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 8e37edb10d183515:0:8e37edb10d183515
0 replace p4: deletion #0, insertion #1
p4 Payment {--is--}{#0 JD}{++falls++}{#1 JD} due within thirty days.
validated 1 unit: no problems, and this file needs no separate check.
wrote edited.docx

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
validated 2 units: no problems, and this file needs no separate check.
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
validated 1 unit: no problems, and this file needs no separate check.

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
validated 2 units: no problems, and this file needs no separate check.
wrote contract.docx

```
