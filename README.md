# stetdoc

stetdoc is a projection layer between Word and AI agents. It turns a `.docx` into a
working copy an agent can read and edit with its ordinary tools, and turns the edits
back into the same `.docx` with real Word tracked changes.

Read the docs at https://stetdoc.clauseo.chat/docs. The [changelog](CHANGELOG.md) lists every release.

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

One `stetdoc` binary and a skill that works in any agent with a shell. Install it with Homebrew:

```
brew install clauseo-ai/tap/stetdoc
```

On a machine without Homebrew, run `uv tool install stetdoc`. [Install](docs/install.md) lists every channel, Windows
included, and [the quickstart](docs/quickstart.md) walks through a first edit in five minutes.

```
# Every revision has an author. Set it here, or save yours once with stetdoc init.
export STETDOC_AUTHOR="Jane Doe"
# Each hit shows its ref, its clause and the exact text.
stetdoc view contract.docx --find 'net 30' --find 'governing law'
# Writes contract.edited.docx. Each OLD must match once, and all pairs apply or none do.
stetdoc replace contract.docx 'net 30' 'net 45'
stetdoc comment contract.edited.docx 'Liability Cap' 'Is this mutual?' --in-place
# A redline of any two versions.
stetdoc compare old.docx new.docx -o redline.docx
```

An edit of a .docx writes `NAME.edited.docx` beside it (then `NAME.edited-2.docx`, never overwriting) and leaves the
input unchanged, unless `-o FILE` names the output or `--in-place` writes back. `replace` matches OLD in the text as it
reads, across formatting. Text that is missing or there more than once is refused with every place listed; `--clause
11.1` or `--range p40..p60` narrows the search, `--ignore-case` ignores case, and `--all` or `--expect N` replaces (or
comments on) every occurrence. `--manual` writes a manual redline instead, old text struck and new text underlined with
no tracked changes, for readers who ask for one. Receipts and views name tracked changes made by others, so the agent can
report them. For longer work, unpack a working copy and edit its readable files:

```
# Writes contract.stet/, which holds the readable files and a hidden complement.
stetdoc unpack contract.docx
# Edit the text after each ref with any tool.
$EDITOR contract.stet/document.md
# Shows the redline that pack will write, validated.
stetdoc check contract.stet
# Writes contract.edited.docx with real tracked changes.
stetdoc pack contract.stet
```

For anything else, such as inserting paragraphs, accepting or rejecting revisions or formatting, agents send a batch of
operations:

```
stetdoc edit contract.stet <<'EOF'
base: 5737b772923cf8d3:0:5737b772923cf8d3
---
@insert
ref: p4
place: after
paragraphs: <<END
A new paragraph after p4.
END
EOF
```

The base is the token the last view or receipt printed. A batch whose base comes from another document or another
history, or that names a block changed since its base, is refused rather than misapplied; a batch from an older state
that touches only blocks unchanged since still applies. `stetdoc compare original.docx revised.docx -o redline.docx` writes a redline of two versions.

The working copy holds readable files (`document.md`, `notes.md`, `headers.md`, `comments.md`) that agents edit with
string replacement or `apply_patch`, and the complement (`.stet/`) that holds everything needed to rebuild the exact
package. Every block has a stable short ref such as `p42`. Every write to a working copy survives a crash, and a second
process waits for the first.

**For agents.** `stetdoc init` installs the skill stub in your home folder for the agents you pick, or for the ones it
detects. It knows Claude Code, Codex, Cursor, OpenCode, OpenClaw, Hermes Agent, Grok Build, cmx2, Antigravity CLI, GitHub
Copilot CLI, Amp and Factory, and still installs for Gemini CLI, which Google retired in favour of Antigravity CLI.
`stetdoc init --project` installs it in a project and adds a short block to `AGENTS.md`. `--list` shows what is installed
and `--remove` takes it out. The full skill is embedded in the binary: `stetdoc skills get core`. For agents that take
tools rather than a shell, `stetdoc mcp` serves the same commands over the Model Context Protocol, and `stetdoc init --mcp`
prints how to register it with each agent. Install the skill with the MCP server, so an agent whose server fails to
start stops rather than editing the XML.

In Claude Code 2.1.275 or later, one line installs the plugin, which carries the skill and a hook that stops Claude from
unzipping a .docx or rewriting its XML (the binary must still be on the PATH; Claude Code uses it rather than MCP):

```
/plugin install stetdoc --marketplace clauseo-ai/stetdoc
```

## Use it with your agent

- [Quickstart](docs/quickstart.md): install, find, replace and comment on a sample contract, then open it in Word.
- [Install](docs/install.md): every channel, Windows, updating and uninstalling.
- [Use stetdoc with your agent](docs/agents.md): `stetdoc init`, the author, and the setup for each agent.
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
// Tracked by default.
let receipt = doc.edit(Edit::ops(batch, stamp))?;
// Unedited, it returns the input bytes.
let saved = doc.save()?;
```

Node and Bun use it through the npm package `@clauseo/stetdoc` (`crates/stetdoc-napi/npm`), with TypeScript types generated
from the same definitions:

```ts
import { Document } from "@clauseo/stetdoc";

const doc = Document.open(bytes);
// Offsets count UTF-16 units.
const receipt = doc.edit({ base: doc.base(), ops, author: "Jane Doe", date });
// Keep it between turns.
const copy = doc.export();
// The backend of Claude's text editor tool.
const result = doc.textEditor(toolUse.input, { author: "Jane Doe", date });
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
