`pack` takes in the hand edits of a working copy, writes `NAME.edited.docx` beside it unless `-o` names another file, and
brings the working copy up to date. It overwrites an existing file only when that file is the document the working copy
came from or one it packed, so a newer version that someone else saved is never lost: `--force` overrides that. The
working copy stays beside the document until you delete the folder, or until a pack with `--done` succeeds and removes it.

```console
$ stetdoc unpack contract.docx
wrote contract.stet: document.md
base 5737b772923cf8d3:0:5737b772923cf8d3
next: stetdoc view contract.stet

$ stetdoc pack contract.stet --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit.
wrote contract.edited.docx
working copy contract.stet is at base 5737b772923cf8d3:0:5737b772923cf8d3. It stays until you delete the folder, or pack again with --done to remove it.

$ stetdoc pack contract.stet -o contract.docx --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit.
wrote contract.docx
working copy contract.stet is at base 5737b772923cf8d3:0:5737b772923cf8d3. It stays until you delete the folder, or pack again with --done to remove it.

$ stetdoc pack contract.stet -o revised.docx --author "Jane Doe"
? 4
error[stale_target]: revised.docx exists and is neither the document contract.stet came from nor one it packed, so packing would overwrite a version it has not seen.
next: Unpack revised.docx again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.

$ stetdoc pack contract.stet -o revised.docx --done --author "Jane Doe"
? 4
error[stale_target]: revised.docx exists and is neither the document contract.stet came from nor one it packed, so packing would overwrite a version it has not seen.
next: Unpack revised.docx again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.

$ stetdoc pack contract.stet --done --author "Jane Doe"
No hand edits were pending, so the document is written as of its last edit.
wrote contract.edited.docx
removed the working copy contract.stet

$ stetdoc view contract.stet
? 3
error[not_found]: There is no file at contract.stet.
next: Check the path, and that the file is readable.

```
