# stetdoc docs

Start here to install stetdoc, connect it to your agent, or build it into a product. The list at the end says what
every page covers.

stetdoc is how AI agents work in Word. Every change your agent makes lands in Word as a tracked change with its name on
it, and everything it didn't touch comes back exactly as it was.

## Start here

**You want your coding agent to edit Word files.** [Install](install.md) stetdoc with one command, run `stetdoc init`,
then follow [your agent's section](agents.md).

**You are an agent told to install stetdoc and use it.** Read [Install](install.md#installing-without-a-terminal-prompt),
then run `stetdoc skills get core` for the full skill.

**You are building a product on stetdoc.** [Build on stetdoc](build-on-stetdoc.md) compares the Rust crate, the Node
package, the CLI and the MCP server.

## What it guarantees

- **Byte identity.** An unedited round trip returns identical bytes. An edited round trip changes only the edited
  elements.
- **No silent changes.** Every change to text, formatting or structure is a Word revision with an author and a date,
  unless you ask for manual markup.
- **Validated before commit.** An edit may not add schema or Word-rule violations, and a batch applies completely or not
  at all.
- **Safe on hostile input.** Bad input fails with a stable error code. stetdoc never panics, hangs or touches the
  network.

## Every page

- [Quickstart](quickstart.md): a first find, replace and comment on a sample contract, then open it in Word.
- [Install](install.md): every channel, Windows, updates, and installing from a script.
- [How stetdoc works](concepts.md): the working copy, refs, base tokens, receipts and refusals.
- [Use stetdoc with your agent](agents.md): `stetdoc init`, the author, and the setup for each agent.
- [Self-hosting the MCP server](self-hosting-mcp.md): serve `stetdoc mcp` to cloud apps, with auth and TLS.
- [Build on stetdoc](build-on-stetdoc.md): the four ways into the engine, for products.
- [stetdoc for Node and Bun](../crates/stetdoc-napi/npm/README.md): the npm package `@clauseo/stetdoc`.
- [CLI reference](reference/cli.md): every command and flag, with examples that run as tests.
- [Operations](reference/operations.md): every operation a batch can hold.
- [Errors](reference/errors.md): every refusal code, its exit status and what to do.
- [Findings](reference/findings.md): every problem `stetdoc check` reports.
- [Fidelity](../FIDELITY.md): per feature, what stetdoc keeps exactly, normalizes, shows read-only or leaves out.
- [Glossary](../GLOSSARY.md): the exact terms.
- [Troubleshooting](troubleshooting.md): fixes for the problems people hit most.
- [Changelog](../CHANGELOG.md): every release.
- [Security](../SECURITY.md): how to report a vulnerability, and how stetdoc tests hostile input.
- [Evals](evals.md): the suite that scores the Claude Code plugin on real legal documents.
