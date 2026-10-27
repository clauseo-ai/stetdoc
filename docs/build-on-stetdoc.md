# Build on stetdoc

This page is for products that put stetdoc inside their own system. There are four ways in. All four
run the same engine, give the same refs and base tokens, and fail with the same stable error codes, so a product can mix
them: a server that edits through the library while an agent in a sandbox uses the CLI on the same documents.

| Way in | Choose it when |
| --- | --- |
| The Rust crate `stetdoc-core` | Your service is in Rust, or you want the engine with no process or runtime around it |
| The Node and Bun package `@clauseo/stetdoc` | Your server is in TypeScript or JavaScript and holds documents in memory between turns |
| The CLI as a subprocess, with `--json` | Any other language, or an agent with a shell in a sandbox |
| MCP, with `stetdoc mcp` | An agent or app that takes tools, with no code of yours in between |

The engine does no file I/O and reads no clock. In the library you pass the bytes, the author and the date. The CLI and
the MCP server read files and take the author from `--author` (or a tool call's `author`), then `STETDOC_AUTHOR`, then
the name a user saved with `stetdoc init`. A product that serves several users passes the author on every call, or sets
it per session (`STETDOC_AUTHOR` in that session's environment, or `stetdoc mcp --author`). It never relies on the config
file, which holds one name for the whole machine. Nothing falls back to git or the operating system.

Nothing has been released yet, so each example below builds from a checkout of the repository. Each one was run against
the same one-paragraph document, `contract.docx`, which reads "Payment is due within thirty days."

## The Rust crate

`stetdoc-core` is one handle, `Document`: open, view, edit, save. Depend on it by path or by git until it is published:

```toml
[dependencies]
stetdoc-core = { git = "https://github.com/clauseo-ai/stetdoc" }
```

```rust
use stetdoc_core::{Batch, Document, Edit, Limits, OpenOptions, Stamp, ViewSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("contract.docx")?;
    let mut doc = Document::open(bytes, &OpenOptions::default())?;
    println!("{}", doc.view(&ViewSpec::default())?.text);

    let batch = Batch::parse(
        &format!("base: {}\n---\n@replace\nref: p1\nfind: thirty days\nwith: sixty days\n", doc.base()),
        &Limits::default(),
    )?;
    let receipt = doc.edit(Edit::ops(batch, Stamp::new("Jane Doe", "2026-10-11T09:00:00")))?;
    println!("{}", receipt.text);

    std::fs::write("contract.edited.docx", doc.save()?.bytes)?;
    Ok(())
}
```

The receipt shows the redline: `p1 Payment is due within {--thirty--}{#0 C}{++sixty++}{#1 C} days.` Every failure is a
`StetError` with a stable `ErrorCode`. Keep the `Document` between turns, or `export` it to a `WorkingCopy` and `resume`
it later, so the agent's refs stay valid. The backend of Claude's text editor tool is the crate `stetdoc-editor`. The
public interface is listed in [`ARCHITECTURE.md`](../ARCHITECTURE.md), and why it has this shape is in
[decision 0001](decisions/0001-core-interface.md).

## The Node and Bun package

`@clauseo/stetdoc` is the engine as a native addon, with TypeScript types generated from the engine's definitions. It needs
Node 22.18 or Bun 1.3.13 or later. Until it is published, build the addon from a checkout with `node scripts/build.mjs`
in `crates/stetdoc-napi/npm`, and depend on that folder.

```js
import { readFileSync, writeFileSync } from "node:fs";
import { Document, StetdocError } from "@clauseo/stetdoc";

const doc = Document.open(readFileSync("contract.docx"));
console.log(doc.view().text); // the page that an agent reads, with its refs

const receipt = doc.edit({
  base: doc.base(),
  ops: [{ op: "replace", ref: "p1", find: "thirty days", with: "sixty days" }],
  author: "Jane Doe",
  date: "2026-10-11T09:00:00",
});
console.log(receipt.text);

const { bytes } = doc.save();
writeFileSync("contract.edited.docx", bytes);

try {
  doc.edit({ base: doc.base(), ops: [{ op: "delete", ref: "p99" }], author: "Jane Doe", date: "2026-10-11T09:00:00" });
} catch (error) {
  if (error instanceof StetdocError) console.log(error.code, error.next); // not_found, and what to do about it
}
```

Calls are synchronous, so a busy server makes them from a worker thread. Offsets count UTF-16 code units, as JavaScript
strings do. `doc.textEditor(input, { author, date })` is the backend of Claude's text editor tool. The full surface is in
[the package's README](../crates/stetdoc-napi/npm/README.md).

## The CLI as a subprocess

Every command takes `--json` and prints its result as JSON on stdout. A failure exits non-zero and prints
`{"error": {"code", "message", "next"}}`. `edit` reads its batch from stdin, so document text never goes through
arguments. In Python:

```python
import json, subprocess

def stetdoc(*args, stdin=None):
    """Run stetdoc with --json and return what it prints. A failure exits non-zero and prints {"error": ...}."""
    run = subprocess.run(["stetdoc", *args, "--json"], input=stdin, capture_output=True, text=True)
    result = json.loads(run.stdout)
    if run.returncode != 0:
        error = result["error"]
        raise RuntimeError(f"{error['code']}: {error['message']} {error['next']}")
    return result

view = stetdoc("view", "contract.docx")  # the page, with the base token a batch must name
batch = f"""base: {view['base']}
---
@replace
ref: p1
find: thirty days
with: sixty days
"""
result = stetdoc("edit", "contract.docx", "-o", "contract.edited.docx",
                 "--author", "Jane Doe", "--date", "2026-10-11T09:00:00", stdin=batch)
print(result["output"], result["receipt"]["blocks"][0]["text"])
```

The exit status tells the class of a failure. The codes and their exit classes are in the
[error reference](reference/errors.md), and every command and flag is in the [CLI reference](reference/cli.md). For
many edits to one document, `unpack` it once to a working copy and edit that, so refs stay stable across calls.

## MCP

`stetdoc mcp` serves the commands as tools on stdin and stdout: `unpack`, `pack`, `view`, `inspect`, `edit`,
`compare`, `check`, `lookup`, `skills_get` and `capabilities`. Each takes the command's flags as arguments and returns
the JSON the command prints with `--json`. The `edit` tool takes its batch in `batch`. A failure is a result with
`isError` set. A client sends one JSON-RPC message per line:

```
$ stetdoc mcp --root ./documents
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"me","version":"0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"view","arguments":{"input":"contract.docx"}}}
```

With `--root`, every path is confined to that folder, and `--author NAME` stamps every call that names no author. To register it with an agent, see [Use stetdoc with your
agent](agents.md). To serve it to a cloud app, see [Self-hosting the MCP server](self-hosting-mcp.md).

## A worked case

A legal drafting product can use two of the four ways together: the CLI and the skill in the
model's sandbox, and `@clauseo/stetdoc` in the server for its own document tools and for Claude's text editor tool, with one
pinned version for both.
