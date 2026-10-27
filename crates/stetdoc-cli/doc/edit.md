A batch in block text, here in `batch.txt`. An agent usually sends it on stdin with a quoted heredoc
(`stetdoc edit contract.stet <<'EOF'` ... `EOF`), so no text needs escaping.

```text
base: 5737b772923cf8d3:0:5737b772923cf8d3
---
@replace
ref: p4
find: thirty days
with: sixty days
```

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ stetdoc edit contract.stet --author "Jane Doe" --ops batch.txt
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:1:6a07baa2d488eeb5
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 1 unit.

```

The same batch on a .docx writes another .docx with `-o`, or the same one with `--in-place`, and `--dry-run` writes nothing.
A dry run needs neither flag, and with either one it ignores it.

```console
$ stetdoc edit contract.docx -o edited.docx --author "Jane Doe" --ops batch.txt --dry-run
--dry-run writes nothing, so -o is ignored.
dry run: nothing was applied. A new block has no ref until the edit is applied. 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 5737b772923cf8d3:0:5737b772923cf8d3
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 1 unit.

```

`--in-place` writes the result back to the .docx it read: through a temporary file beside it and a rename, so a crash
leaves the old document or the new one, and only if no one wrote the file during the edit (`stale_target` otherwise).
A symbolic link is written through and stays a link. A hard link is broken, because the rename puts a new file at the
path. The permissions are kept, the owner is not, and a read-only file is refused. On Unix the folder is synced after the
rename; other systems cannot sync a folder, so there the rename is as durable as the file system makes it. A saved
document opens in a new ref namespace, so the receipt's base is the written file's: the next batch can start from
it without a view. When the edit added or removed blocks, the written file numbers them again, so the receipt has no
base and its `next` says to view the file. A batch from before the write is stale, as the second command shows.

```console
$ stetdoc edit contract.docx --in-place --author "Jane Doe" --ops batch.txt
applied 1 operation, tracked as Jane Doe 2026-10-06T09:30:00Z. base 745747bfd39463c1:0:745747bfd39463c1
0 replace p4: deletion #0, insertion #1
p4 Payment is due within {--thirty--}{#0 JD}{++sixty++}{#1 JD} days.
checked 1 unit.
wrote contract.docx

$ stetdoc edit contract.docx --in-place --author "Jane Doe" --ops batch.txt
? 4
error[stale_ref]: This batch was written for another document (namespace 5737b772923cf8d3, not 745747bfd39463c1). Refs do not carry over when a file is reopened. View the document again. (operation 0)
next: If this file was rewritten since that view or receipt, by an edit that wrote it or by someone else, its refs are new: view it again, then rewrite the batch from what it shows.

```
