# stetdoc for Node and Bun

stetdoc turns a `.docx` into a working copy that an agent can read and edit, and compiles the edits back into the same
package as real Word tracked changes. Everything the edits did not touch is kept byte for byte. This package is the
library: the stetdoc engine as a native addon, with TypeScript types generated from the engine's own definitions.

```ts
import { Document, compare } from "@stetdoc/node";

const doc = Document.open(bytes);                                  // a Uint8Array of a .docx
const view = doc.view();                                           // the body, or its outline when it is long
const receipt = doc.edit({
  base: doc.base(),
  ops: [{ op: "replace", ref: "p4", find: "thirty", with: "sixty" }],
  author: "Jane Doe",
  date: "2026-10-06T10:00:00",
});                                                                // tracked by default
const { bytes: saved } = doc.save();                               // unedited, it returns the input bytes
const copy = doc.export();                                         // keep it between turns
const again = Document.resume(copy);                               // refs such as p4 stay valid
const { document: redline, receipt: diff } = compare(original, revised, { stamp: { author: "Jane Doe", date } });
```

## Requirements

Node 22.18 or later, or Bun 1.3.13 or later, on one of seven targets: `linux-x64-gnu`, `linux-arm64-gnu`,
`linux-x64-musl`, `linux-arm64-musl`, `darwin-x64`, `darwin-arm64` and `win32-x64-msvc`. The addon for each target is its
own package, `@stetdoc/<target>`, which the package manager installs as an optional dependency. No package runs a script
on install. A missing addon throws an error that names the target and the paths it tried.

## The rules

- **Calls are synchronous**: bytes in, bytes out. A worst-case call on a 300-page document takes about a second, so a
  server that cares about its event loop makes its calls from a worker thread. The addon loads in Node and Bun worker
  threads.
- **One `Document` per document, on one thread.** A `Document` belongs to the thread that made it. Serialize the edits to
  it, and keep it (or its `WorkingCopy`) between turns. Reopening saved bytes gives new refs, so an agent's refs would go
  stale.
- **No file I/O and no clock.** The caller supplies the bytes, the author and the date (`YYYY-MM-DDTHH:MM:SS`, the
  user's wall clock).
- **Offsets count UTF-16 code units**, as a JavaScript string does, unless an edit says `offsets: "scalar"`.
- **Every failure is a thrown `StetdocError`**, except in `textEditor`, whose failures are text for the model.

## `Document`

| Method | Returns | What it does |
|---|---|---|
| `Document.open(bytes, options?)` | `Document` | Opens a package. `options.limits` bounds untrusted input; `options.format` is the working-copy format (`stet/1` or `json/1`) |
| `Document.resume(workingCopy, options?)` | `Document` | Resumes a working copy that `export` returned, with every ref still valid |
| `base()` | `string` | The base token that a batch names |
| `view(spec?)` | `View` | A page within a token budget: `{ scope, mode, budget, after, format, notes }`, every field optional. `{ budget: 0, mode: "final", format: "json/1" }` is the whole Final text as JSON |
| `edit(request)` | `Receipt` | Applies `ops` (with `base`), a `batch` string in JSON or block text, or hand-edited `files`, signed by `author` and `date`. Options: `untracked`, `scope` (refs and ranges such as `p40..p60`), `dryRun`, `quotes` (`"match"` or `"as_typed"`), `offsets` |
| `save()` | `{ bytes, receipt }` | The package and a summary of it. An unedited document returns its input bytes exactly |
| `inspect(target)` | `Inspection` | One block (`p42`, `t3`, `r17`), or `styles`, `numbering` or `sections` |
| `revisions(filter?)` | `RevisionInfo[]` | The tracked changes, filtered by `author` and `kind` |
| `lookup(query)` | `Lookup` | A defined term, a clause number or a cross-reference, resolved |
| `check()` | `CheckReport` | Schema and Word-rule problems of the document as it is |
| `checkPending(request?)` | `CheckResult` | A dry run of `pending` readable files, the revisions already there and the check report |
| `files()` | `ReadableFiles` | The readable files, as `export` would write them |
| `export()` | `WorkingCopy` | `{ files: [name, text][], complement: [name, Uint8Array][] }`: plain data to store |
| `textEditor(input, options)` | `TextEditorResult` | One call of Claude's text editor tool, below |

The operations, their fields and the error codes are listed by `capabilities()`, and every data type is in `types.d.ts`.

## Claude's text editor tool

`textEditor` is the backend of Anthropic's `text_editor_20250728` tool (tool name `str_replace_based_edit_tool`) over the
document's readable files. Pass the `tool_use` block's input exactly as the model sent it, as an object or its JSON:

```ts
const result = doc.textEditor(toolUse.input, { author: "Jane Doe", date, pathPrefix: "/documents/acme/" });
// { content: string, isError: boolean, committed: boolean, receipt?: Receipt }
return { type: "tool_result", tool_use_id: toolUse.id, content: result.content, is_error: result.isError };
```

- `view` shows a file with line numbers, or with the prefix (or `/`) lists the files. `view_range` is supported.
- `str_replace` needs `old_str` to match exactly once. Otherwise the result is `not_found`, or `ambiguous_match` with
  each match and a line of context on each side.
- `str_replace` and `insert` commit at once as one tracked edit, and show the touched blocks where they now are.
- `create` and `undo_edit` are `unsupported`. A path outside `pathPrefix` is `not_found`.
- A failure changes nothing, and comes back as text with `isError` set, never as a thrown error.

## Errors

```ts
try {
  doc.edit(request);
} catch (error) {
  if (error instanceof StetdocError) {
    error.code;     // stable from 1.0, such as "stale_ref" or "ambiguous_match"
    error.message;  // one line
    error.next;     // what to do about it
    error.opIndex;  // the failing operation of a batch, when there is one
    error.file;     // the readable file and the line of a failing edit of files, when there is one
    error.line;
  }
}
```

`JSON.stringify(error)` gives the wire shape that the CLI's `--json` prints. The codes, their meanings and their exit
classes are in the repository's [error reference](../../../docs/reference/errors.md), and `capabilities().error_codes`
lists them at run time.

## Versions

`version` is the release the addon was built from, to pin against the CLI's `stetdoc --version`. `formatVersion` is the
version of the working copy's complement that this release writes and reads.

## Licence

MIT or Apache-2.0, at your option.
