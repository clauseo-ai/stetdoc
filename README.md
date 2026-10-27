# stetdoc

stetdoc is a projection layer between Word and AI agents. It turns a `.docx` into a
working copy an agent can read and edit with its ordinary tools, and turns the edits
back into the same `.docx` with real Word tracked changes.

**Status:** under development toward 1.0.0. Nothing has been released yet. The CLI below
works today, `compare` included. The [changelog](CHANGELOG.md) lists what is done so far.

## Why

Word's file format is hostile to agents. Text is fragmented across runs, tracked changes
are XML structure, the clause numbers lawyers cite ("4.2(a)") are computed and appear
nowhere in the file, and Word rejects a file whose XML is slightly out of schema order.
Agents that edit raw XML are slow and fragile, and agents that round-trip through
Markdown lose formatting, tracked changes and comments.

## What it guarantees

- **Byte identity.** An unedited round trip returns identical bytes. An edited round trip
  changes only the edited elements.
- **No silent changes.** Editing is tracked by default: every change to text, formatting
  or structure is a Word revision with an author and date.
- **Validated before commit.** An edit may not introduce schema or Word-rule violations,
  and a batch applies completely or not at all.
- **Safe on hostile input.** Bad input fails with a stable error code. It never panics,
  hangs or touches the network.

## Usage

One `stetdoc` binary and a skill that works in any agent with a shell. Until the first release, build it from this
repository with `cargo install --path crates/stetdoc-cli`.

```
export STETDOC_AUTHOR="Jane Doe"          # every revision has an author (or save yours once with stetdoc init)
stetdoc unpack contract.docx             # writes contract.stet/: readable files and a hidden complement
stetdoc view contract.stet               # the body, or an outline of a large one; --range, --find, --revisions
$EDITOR contract.stet/document.md        # edit the text after each ref with any tool
stetdoc check contract.stet              # the redline that pack will write, validated
stetdoc pack contract.stet               # writes contract.edited.docx with real tracked changes
```

Agents can also send operations, which keep document text out of arguments:

```
stetdoc edit contract.stet <<'EOF'
base: 5737b772923cf8d3:0:5737b772923cf8d3
---
@replace
ref: p4
find: thirty days
with: sixty days
EOF
```

The base is the token the last view or receipt printed. A batch whose base comes from another document or another
history, or that names a block changed since its base, is refused rather than misapplied; a batch from an older state
that touches only blocks unchanged since still applies. `stetdoc compare original.docx revised.docx -o redline.docx` writes a redline of two versions.

The working copy holds readable files (`document.md`, `notes.md`, `headers.md`, `comments.md`) that agents edit with
string replacement or `apply_patch`, and the complement (`.stet/`) that holds everything needed to rebuild the exact
package. Every block has a stable short ref such as `p42`. Every write to a working copy survives a crash, and a second
process waits for the first.

**For agents.** `stetdoc init` installs the skill stub in your home folder for the agents you pick (Claude Code, Codex,
Cursor, OpenCode, Gemini CLI, Copilot, Amp, OpenClaw, Hermes, Grok Build, cmx2, Factory), or for the ones it detects;
`stetdoc init --project` installs it in a project and adds a short block to `AGENTS.md`. `--list` shows what is installed
and `--remove` takes it out. The full skill is embedded in the binary: `stetdoc skills get core`. For agents that take
tools rather than a shell, `stetdoc mcp` serves the same commands over the Model Context Protocol, and `stetdoc init --mcp`
prints how to register it with each agent. Install the skill with the MCP server, so an agent whose server fails to
start stops rather than editing the XML.

In Claude Code 2.1.275 or later, one line installs the plugin, which carries the skill and the MCP server (the binary
must still be on the PATH):

```
/plugin install stetdoc --marketplace clauseo-ai/stetdoc
```

## Use it with your agent

- [Use stetdoc with your agent](docs/agents.md): install, `stetdoc init`, and the setup for each agent.
- [Self-hosting the MCP server](docs/self-hosting-mcp.md): serve `stetdoc mcp` to cloud apps, with auth and TLS.
- [Build on stetdoc](docs/build-on-stetdoc.md): the Rust crate, the Node package, the CLI and MCP, for products.

**Reference.** The [CLI reference](docs/reference/cli.md) lists every command and flag, with examples that run as
tests. [`FIDELITY.md`](FIDELITY.md) says, per feature, what is kept exactly, what is normalized, what is read-only and
what is left out.

## The library

The same engine is a Rust library, `stetdoc-core`, built around one handle:

```rust
let mut doc = Document::open(bytes, &OpenOptions::default())?;
let view = doc.view(&ViewSpec::default())?;
let receipt = doc.edit(Edit::ops(batch, stamp))?;   // tracked by default
let saved = doc.save()?;                            // unedited => the input bytes
```

Node and Bun use it through the npm package `@stetdoc/node` (`crates/stetdoc-napi/npm`), with TypeScript types generated
from the same definitions:

```ts
import { Document } from "@stetdoc/node";

const doc = Document.open(bytes);
const receipt = doc.edit({ base: doc.base(), ops, author: "Jane Doe", date });  // offsets count UTF-16 units
const copy = doc.export();                                                      // keep it between turns
const result = doc.textEditor(toolUse.input, { author: "Jane Doe", date });     // Claude's text editor tool
```

Its documentation is [`crates/stetdoc-napi/npm/README.md`](crates/stetdoc-napi/npm/README.md). The backend of Claude's
text editor tool is the Rust crate `stetdoc-editor`. Errors carry a stable code, a message and a `next` hint, and the
library throws them as `StetdocError`. See the [error reference](docs/reference/errors.md).

## Repository

- [`ARCHITECTURE.md`](ARCHITECTURE.md): the map for anyone new to the code
- [`GLOSSARY.md`](GLOSSARY.md): the exact domain terms
- [`docs/decisions/`](docs/decisions/0001-core-interface.md) and [`docs/design/`](docs/design/revisions.md): the
  decision records and the design notes behind each module
- [`docs/agents.md`](docs/agents.md), [`docs/self-hosting-mcp.md`](docs/self-hosting-mcp.md) and
  [`docs/build-on-stetdoc.md`](docs/build-on-stetdoc.md): using stetdoc with agents, cloud apps and products
- [`AGENTS.md`](AGENTS.md) and [`CONTRIBUTING.md`](CONTRIBUTING.md): how to work here
- [`FIDELITY.md`](FIDELITY.md): what is kept exactly, normalized, read-only or left out
- [`SECURITY.md`](SECURITY.md): how to report a vulnerability, and how hostile input is tested
- [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md): how we expect people to treat each other here

Build and verify with `cargo xtask check`. Besides formatting, lints and unit tests, it runs the
laws on a curated corpus of about 300 documents: every unedited round trip is byte-identical, and
50 seeded random edit batches per document keep their laws (locality, validity, accept and reject
restore the expected text). The full corpora, about 48,000 documents, run nightly in CI, and each
fuzz target runs for 24 hours before a release.

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT licence](LICENSE-MIT), at your option. See [`NOTICE`](NOTICE).
