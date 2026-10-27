# Use stetdoc with your agent

stetdoc is one binary and one skill. The skill teaches an agent to find text in a `.docx` with `stetdoc view --find`,
change it with `stetdoc replace`, comment with `stetdoc comment`, and use a batch of operations or a working copy for
the rest, all as real tracked changes. Any agent with a shell can use it that way.
Agents that take tools rather than a shell can use `stetdoc mcp`, which serves the same commands over the Model Context
Protocol. Claude Code uses the command and the skill, not MCP.

This page covers installing the binary, installing the skill with one command, and then each agent: where the skill
goes, how to register the MCP server if the agent uses one, and how to check that the agent sees it. Each section links
the agent's own docs, which were read on 2026-10-11. Agents change their folders and config files, so trust their docs
over this page when the two disagree.

## Install stetdoc

Nothing has been released yet. Until the first release, build from a checkout of the repository:

```
cargo install --path crates/stetdoc-cli
```

Once releases exist, any of these works.

| Channel | Command | Notes |
| --- | --- | --- |
| Homebrew | `brew install clauseo-ai/stetdoc/stetdoc` | macOS and Linux, from the tap `clauseo-ai/homebrew-stetdoc` |
| npm | `npx stetdoc --version` | Runs the binary for your platform without installing it. `npm install -g stetdoc` puts it on the PATH |
| Install script | `curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh \| sh` | macOS and Linux. Checks the SHA-256 and installs to `~/.local/bin`, or `$STETDOC_INSTALL_DIR` |
| Cargo | `cargo install --path crates/stetdoc-cli` | Works today, from a checkout |

[`packaging/README.md`](../packaging/README.md) says how each channel is built and published.

Every revision needs an author. `stetdoc init` sets it once (see below).

## The one command

```
stetdoc init             # the skill stub in your home folder, for the agents you pick
stetdoc init --project   # the skill stub in this project, plus a short block in AGENTS.md
```

In a terminal, `init` asks which agents to install for. Otherwise it installs for the agents it detects. `--agent`
names them instead, such as `--agent claude-code,codex` or `--agent all`. `--dry-run` lists what it would write and
changes nothing. Running it again is safe: a file with the same bytes is left as it is.

The stub is short. It carries the everyday commands (find, replace, comment, compare), which cover a simple task, and
tells the agent to run `stetdoc skills get core` for anything more, which prints the full skill embedded in the binary,
so the skill always matches the version you run. In Claude Code the stub sends the agent straight to the command. In
another agent with stetdoc's MCP tools, it sends the agent to the `skills_get` tool instead.

```
stetdoc init --list        # each agent, whether it is detected, its skill folder, and the stetdoc version there
stetdoc init --mcp         # also print how to register `stetdoc mcp` with each agent
stetdoc init --remove      # remove the skill folders stetdoc wrote, and nothing else
```

`init` never edits an agent's own config. `--mcp` prints the line to add, and the sections below give the same lines.
This is the real output of `stetdoc init --agent all --mcp --dry-run` in an empty home folder:

```
would write ~/.claude/skills/stetdoc/SKILL.md (Claude Code)
would write ~/.agents/skills/stetdoc/SKILL.md (Codex, Cursor, OpenCode, OpenClaw, Gemini CLI, GitHub Copilot CLI, Amp)
would write ~/.gemini/config/skills/stetdoc/SKILL.md (Antigravity CLI)
would write ~/.hermes/skills/stetdoc/SKILL.md (Hermes Agent)
would write ~/.grok/skills/stetdoc/SKILL.md (Grok Build)
would write ~/.config/cmx2/skills/stetdoc/SKILL.md (cmx2)
would write ~/.factory/skills/stetdoc/SKILL.md (Factory Droid)

To use stetdoc as an MCP server as well, keep the skill installed with it: without the skill, an agent whose MCP server fails to start may edit the .docx's XML itself.
Register `stetdoc mcp` with each agent (init edits no agent's config):
  Claude Code: claude mcp add --scope user stetdoc -- stetdoc mcp
  Codex: codex mcp add stetdoc -- stetdoc mcp
  Cursor: in ~/.cursor/mcp.json: { "mcpServers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } }
  OpenCode: in ~/.config/opencode/opencode.json: { "mcp": { "stetdoc": { "type": "local", "command": ["stetdoc", "mcp"], "enabled": true } } }
  OpenClaw: openclaw mcp add stetdoc --command stetdoc --arg mcp (or in ~/.openclaw/openclaw.json: { "mcp": { "servers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } } })
  Gemini CLI: in ~/.gemini/settings.json: { "mcpServers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } }
  Antigravity CLI: agy mcp add stetdoc stetdoc mcp (or in ~/.gemini/config/mcp_config.json: { "mcpServers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } })
  GitHub Copilot CLI: in ~/.copilot/mcp-config.json: { "mcpServers": { "stetdoc": { "type": "local", "command": "stetdoc", "args": ["mcp"], "tools": ["*"] } } }
  Amp: amp mcp add stetdoc -- stetdoc mcp
  Hermes Agent: in ~/.hermes/config.yaml: mcp_servers: { stetdoc: { command: stetdoc, args: [mcp] } }
  Grok Build: grok mcp add stetdoc -- stetdoc mcp
  cmx2: in ~/.config/cmx2/cmx2.jsonc: "mcp": { "stetdoc": { "command": ["stetdoc", "mcp"] } }
  Factory Droid: in ~/.factory/mcp.json: { "mcpServers": { "stetdoc": { "type": "stdio", "command": "stetdoc", "args": ["mcp"] } } }
```

Several agents read the shared folder `~/.agents/skills` (`.agents/skills` in a project) and their own folder or
`~/.claude/skills` too, so an agent may list the stub more than once. The copies are identical, and `init` prints a note
for each agent this applies to.

### The author

Every revision carries an author, and stetdoc never makes one up. In a terminal, `stetdoc init` asks for your name once,
suggesting your git `user.name`, and saves the answer to `stetdoc/config.toml` in your config folder (`XDG_CONFIG_HOME`,
else `%APPDATA%` on Windows, else `~/.config`). `stetdoc init --author "Your Name"` saves it without asking, and
`stetdoc init --list` shows the saved name. Without a terminal and without `--author`, `init` saves nothing and says how
to set one.

When a command writes revisions, the author is the first of:

1. `--author` on the command line, or the `author` argument of an MCP tool call (`stetdoc mcp --author` sets it for
   every call that names none),
2. `STETDOC_AUTHOR` in the environment,
3. the name saved with `stetdoc init`.

With none of them, the command fails and says to run `stetdoc init`. Git and the operating system are never asked at
edit time: the git name is only a suggestion in `init`'s question.

### What `init` replaces and removes

A `SKILL.md` counts as stetdoc's only when it is a stub that some stetdoc build wrote, unchanged. `init` replaces such a
stub freely. Anything else, such as a stub you edited, your own skill named `stetdoc`, or a `SKILL.md` that is a link, is
replaced only with `--force`. Without it, `init` refuses and writes nothing.

`--remove` deletes a skill folder only when it holds nothing but an unchanged stub, and never its parent. It never removes
anything else, even with `--force`. When you name an agent with `--agent` and its folder fails that test, nothing is
removed. Without `--agent`, every agent's folder is tried and the others are left with a note. In a project, the
`AGENTS.md` block goes too. `--remove` leaves the saved author alone.

### A note on MCP

Install the skill together with the MCP server, or a plugin that carries both, rather than the server alone. The skill
tells the agent which surface to use: the `stetdoc` command in Claude Code, elsewhere the MCP tools when they are there,
else the command. When neither works,
it tells the agent to say that stetdoc is not installed and stop. Without the skill, an agent whose MCP server fails to
start has nothing that keeps it away from the package, and in testing it fell back to unzipping the .docx and reading its
XML with its own scripts.

`stetdoc mcp` speaks MCP on stdin and stdout. Every snippet below runs it as a local stdio server. On your own machine
the server finds the author you saved with `stetdoc init`, so the snippets need nothing more. To use another name for one
agent, add `--author "Name"` after `mcp` in its snippet. Before the binary is on the PATH, `npx stetdoc mcp` works in
place of `stetdoc mcp` once the npm package is published.

## Claude Code

- **Skill:** `~/.claude/skills/stetdoc/SKILL.md`, or `.claude/skills/stetdoc/SKILL.md` in a project. `stetdoc init
  --agent claude-code` writes it.
- **Or the plugin**, which carries the skill and the [guard hook](#the-claude-code-guard-hook) (the binary must still be
  on the PATH). It has no MCP server: in Claude Code the command and the skill are the route. Claude Code keeps MCP tools
  behind its tool search, so with both an agent switched between them and the command, and the server is the same binary,
  so it is no fallback when the command is missing. In a session of Claude Code 2.1.275 or later, one line installs it:

  ```
  /plugin install stetdoc --marketplace clauseo-ai/stetdoc
  ```

  On any version, two commands from the shell do the same:

  ```
  claude plugin marketplace add clauseo-ai/stetdoc
  claude plugin install stetdoc@stetdoc
  ```

- **MCP:** not needed, and the command works better. If you want it anyway: `claude mcp add --scope user stetdoc --
  stetdoc mcp`, or `--scope project` for a project.
- **Check:** type `/stetdoc` in a session to load the skill. `/hooks` lists the plugin's PreToolUse hook.
- Docs: [skills](https://code.claude.com/docs/en/skills), [hooks](https://code.claude.com/docs/en/hooks),
  [plugins](https://code.claude.com/docs/en/plugins).

### The Claude Code guard hook

When stetdoc is missing, or its view does not answer a question, an agent may reach for the package itself: unzip the
.docx and read or rewrite `word/document.xml` with a script. Such an edit is not a tracked change, and it can break the
file. The plugin carries a PreToolUse hook, `plugins/claude/hooks/docx-guard.sh`, that blocks those tool calls and tells
the agent to use stetdoc, or to tell the user stetdoc is not installed. It needs only `sh` and `awk`, never stetdoc.

It blocks:

- an archive tool (`unzip`, `zip`, `zipinfo`, `ditto`, `tar`, `bsdtar`, `7z`, `jar`, `unar`, `Expand-Archive` and
  the like) in a command that names a .docx, .docm, .dotx or .dotm file;
- a script (`python`, `node`, `ruby`, `perl`, `php`, `osascript` and the like) run on such a file, and zip code
  (`zipfile`, `JSZip`, `adm-zip`, `Archive::Zip`, `java.util.zip`) in a command that names one;
- a library that reads or writes .docx files directly, used or installed: `python-docx` (`import docx`), `docx2python`,
  `docx2txt`, `mammoth`, `docxtpl`, the `docx` npm package, `docxtemplater`, Apache POI, Open XML SDK and Aspose;
- a conversion with `textutil`, `pandoc`, `soffice` or `libreoffice` in a command that names a Word file;
- copying a Word file to a `.zip` name, `sed -i` or `perl -i` on a Word file or an unzipped part, and `dd` or `xxd` on a
  Word file;
- reading or writing an unzipped part (`word/*.xml`, `document.xml`, `[Content_Types].xml`) with a script, `sed`, `awk`,
  `cat`, `xmllint` or a redirection;
- the Write, Edit, MultiEdit and NotebookEdit tools on a Word file or an unzipped part, and a script written to `/tmp` that
  takes a Word file apart.

It allows every `stetdoc` command, whatever its arguments or here-document say, and commands that only list, copy, move,
hash or open a Word file (`ls`, `cp`, `mv`, `shasum`, `open`, `file`, `rm`). `git` and `gh` commands are not checked,
because their messages are prose. When it is unsure, it allows: it reads the command, not what a script file holds, so a
script written earlier inside the project and run without naming a Word file passes. `STETDOC_GUARD=off` in Claude Code's
environment turns it off.

Without the plugin, add the hook to a skill install: save the script, then add it to `~/.claude/settings.json` (or a
project's `.claude/settings.json`). `stetdoc init --agent claude-code` points here.

```
mkdir -p ~/.claude/hooks
curl -fsSL -o ~/.claude/hooks/stetdoc-docx-guard.sh \
  https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/plugins/claude/hooks/docx-guard.sh
```

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash|PowerShell|Write|Edit|MultiEdit|NotebookEdit",
        "hooks": [{ "type": "command", "command": "sh ~/.claude/hooks/stetdoc-docx-guard.sh", "timeout": 30 }]
      }
    ]
  }
}
```

## Codex

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md`, or `.agents/skills/stetdoc/SKILL.md` in a repository.
- **MCP:** `codex mcp add stetdoc -- stetdoc mcp`. It writes `[mcp_servers.stetdoc]` in `~/.codex/config.toml`.
- **Check:** `/skills` in a session, or type `$` to mention a skill. `codex mcp list`, or `/mcp` in a session.
- Docs: [skills](https://developers.openai.com/codex/skills), [MCP](https://developers.openai.com/codex/mcp).

## Cursor

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md`, or `.agents/skills/stetdoc/SKILL.md` in a project. Cursor also reads
  `~/.cursor/skills` and the Claude and Codex folders.
- **MCP:** in `~/.cursor/mcp.json`, or `.cursor/mcp.json` in a project:

  ```json
  { "mcpServers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } }
  ```

- **Check:** open Customize in the sidebar. Skills lists the stub, and MCP lists the server.
- Docs: [skills](https://cursor.com/docs/context/skills), [MCP](https://cursor.com/docs/mcp).

## OpenCode

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md`, or `.agents/skills/stetdoc/SKILL.md` in a project. OpenCode also reads
  `.claude/skills`, so it sees the Claude Code copy as well.
- **MCP:** in `~/.config/opencode/opencode.json`, or `opencode.json` in a project:

  ```json
  { "mcp": { "stetdoc": { "type": "local", "command": ["stetdoc", "mcp"], "enabled": true } } }
  ```

- **Check:** ask the agent which skills it has. OpenCode lists them in its `skill` tool. `opencode mcp list` shows the
  server.
- Docs: [skills](https://opencode.ai/docs/skills), [MCP servers](https://opencode.ai/docs/mcp-servers).

## OpenClaw

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md`. OpenClaw reads this folder in its default state. A workspace's own
  `skills/` folder comes first, and `~/.openclaw/skills` is also read.
- **MCP:** `openclaw mcp add stetdoc --command stetdoc --arg mcp`, or by hand in `~/.openclaw/openclaw.json`:

  ```json
  { "mcp": { "servers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } } }
  ```

- **Check:** `openclaw skills list --eligible` lists the stub. `openclaw mcp probe stetdoc` connects and lists the tools.
- Docs: [skills](https://docs.openclaw.ai/tools/skills), [MCP](https://docs.openclaw.ai/cli/mcp).

## Hermes Agent

- **Skill:** `~/.hermes/skills/stetdoc/SKILL.md`, or `.hermes/skills/stetdoc/SKILL.md` in a project. Hermes reads a
  project's skills only after you run `hermes skills trust` there, and then it reads `.agents/skills` too.
- **MCP:** in `~/.hermes/config.yaml`:

  ```yaml
  mcp_servers:
    stetdoc:
      command: stetdoc
      args: [mcp]
  ```

- **Check:** `hermes skills list`. Each skill is also a slash command, so `/stetdoc` works in a session. After an MCP
  change, run `/reload-mcp` or start a new session.
- Docs: [skills](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills),
  [MCP](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp).

## Grok

**Grok Build** (the CLI):

- **Skill:** `~/.grok/skills/stetdoc/SKILL.md`, or `.grok/skills/stetdoc/SKILL.md` in a project.
- **MCP:** `grok mcp add stetdoc -- stetdoc mcp`. Add `--scope project` to write `.grok/config.toml` instead.
- **Check:** `/skills` or `/mcps` in the TUI. `grok mcp list` and `grok mcp doctor stetdoc` from the shell.
- Docs: [skills, plugins and marketplaces](https://docs.x.ai/build/features/skills-plugins-marketplaces.md),
  [MCP servers](https://docs.x.ai/build/features/mcp-servers.md).

**Grok Bot** works from its own persistent cloud computer with a command line, so it can install stetdoc there itself.
Once releases exist, ask it to run the install script (or `npx stetdoc`), then `stetdoc init --agent grok` or simply
`stetdoc skills get core` before it works on a `.docx`. The files it edits live on that cloud computer, so upload the
document to it and download the result. Docs: [computer and apps](https://docs.x.ai/grok-bot/computer-and-apps.md).

## cmx2

- **Skill:** `~/.config/cmx2/skills/stetdoc/SKILL.md`, or `.cmx2/skills/stetdoc/SKILL.md` in a project. cmx2 also reads
  `~/.agents/skills` and `~/.claude/skills`.
- **MCP:** under `mcp` in `~/.config/cmx2/cmx2.jsonc`:

  ```jsonc
  "mcp": { "stetdoc": { "command": ["stetdoc", "mcp"] } }
  ```

- **Check:** `cmx2 mcp` lists each server with its status and tool count. The tools appear in sessions as
  `stetdoc_<tool>`. Ask a session which skills it has to see the stub.
- Docs: `cmx2 mcp --help`. These folders were read from cmx2 0.8.11.

## Antigravity CLI

Antigravity CLI (`agy`) is Google's terminal agent, which replaced Gemini CLI. `init` detects it by `agy` on the PATH,
or by `~/.gemini/antigravity-cli` or `~/.gemini/antigravity`.

- **Skill:** `~/.gemini/config/skills/stetdoc/SKILL.md`, or `.agents/skills/stetdoc/SKILL.md` in a workspace.
  Antigravity CLI, the Antigravity app and the Antigravity IDE all read `~/.gemini/config/skills`. The CLI's own docs name
  `~/.gemini/antigravity-cli/skills` too, which only the CLI and the IDE read. Antigravity CLI 1.4.8 does not read
  `~/.agents/skills`, as a test showed. It reads `AGENTS.md` and `GEMINI.md` alike, so `stetdoc init --project`'s block in `AGENTS.md`
  reaches it.
- **MCP:** in `~/.gemini/config/mcp_config.json`, which the CLI, the app and the IDE share:

  ```json
  { "mcpServers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } }
  ```

  `agy mcp add stetdoc stetdoc mcp` writes the same entry. For a stetdoc server you host yourself (see
  [self-hosting the MCP server](self-hosting-mcp.md)), give its address as `serverUrl`, not `url`, which Antigravity
  ignores (`agy mcp add --header "Authorization: Bearer TOKEN" stetdoc https://stetdoc.example.com/mcp` writes it):

  ```json
  { "mcpServers": { "stetdoc": { "serverUrl": "https://stetdoc.example.com/mcp", "headers": { "Authorization": "Bearer TOKEN" } } } }
  ```

  The docs also name a workspace file, `.agents/mcp_config.json`, but Antigravity CLI reads it and then ignores its
  servers ([antigravity-cli#60](https://github.com/google-antigravity/antigravity-cli/issues/60)), so
  `init --project --mcp` prints the global file too.
- **Check:** `/skills` in a session lists the skills and where each comes from. `/mcp` opens the MCP manager.
- Docs: [skills](https://antigravity.google/docs/skills), [MCP](https://antigravity.google/docs/mcp), read on 2026-10-27.

## Gemini CLI (retired)

Google retired Gemini CLI on 18 June 2026 in favour of Antigravity CLI (see above). `init` keeps it for those who still
run it, and says so when it is picked. Install for Antigravity with `stetdoc init --agent antigravity`.

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md` (Gemini CLI reads it as an alias of `~/.gemini/skills`), or
  `.agents/skills/stetdoc/SKILL.md` in a workspace.
- **MCP:** in `~/.gemini/settings.json`, or `.gemini/settings.json` in a project:

  ```json
  { "mcpServers": { "stetdoc": { "command": "stetdoc", "args": ["mcp"] } } }
  ```

  `gemini mcp add stetdoc stetdoc mcp` writes the same entry.
- **Check:** `/skills list` in a session, or `gemini skills list --all`. `gemini mcp list`, or `/mcp` in a session.
- Docs: [skills](https://geminicli.com/docs/cli/skills), [MCP servers](https://geminicli.com/docs/tools/mcp-server/).

## GitHub Copilot CLI

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md` (Copilot also reads `~/.copilot/skills`), or
  `.agents/skills/stetdoc/SKILL.md` in a repository.
- **MCP:** in `~/.copilot/mcp-config.json`:

  ```json
  { "mcpServers": { "stetdoc": { "type": "local", "command": "stetdoc", "args": ["mcp"], "tools": ["*"] } } }
  ```

  `copilot mcp add stetdoc -- stetdoc mcp` writes it for you.
- **Check:** `/mcp list` or `/mcp show stetdoc` in a session. Ask the agent which skills it has to see the stub.
- Docs: [skills](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-skills),
  [MCP servers](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers).

## Amp

- **Skill:** `~/.agents/skills/stetdoc/SKILL.md`, or `.agents/skills/stetdoc/SKILL.md` in a project.
- **MCP:** `amp mcp add stetdoc -- stetdoc mcp`.
- **Check:** ask the agent which skills and tools it has.
- Docs: [skills](https://ampcode.com/docs/markdown/customize/skills), [MCP](https://ampcode.com/docs/markdown/customize/mcp).

## Factory

- **Skill:** `~/.factory/skills/stetdoc/SKILL.md`, or `.factory/skills/stetdoc/SKILL.md` in a repository. Droid also
  reads `~/.agents/skills`.
- **MCP:** in `~/.factory/mcp.json`, or `.factory/mcp.json` in a project:

  ```json
  { "mcpServers": { "stetdoc": { "type": "stdio", "command": "stetdoc", "args": ["mcp"] } } }
  ```

  `droid mcp add stetdoc stetdoc mcp` writes a stdio server as well.
- **Check:** `/mcp` in a session shows the server. Ask the agent which skills it has to see the stub.
- Docs: [skills](https://docs.factory.ai/cli/configuration/skills), [MCP](https://docs.factory.ai/cli/configuration/mcp).

## Dots (ChatGPT desktop)

A dot has its own cloud computer. With local computer access turned on in the ChatGPT desktop app on your machine, it
can work on that computer's files and use local skills. So install stetdoc on the connected computer and run `stetdoc
init` there. OpenAI's article does not name the folder a dot reads skills from. The Codex folder, `~/.agents/skills`, is
the likely one, because a dot starts Codex tasks, but this is not confirmed. The article does not mention MCP. Docs:
[OpenAI help article 20001530](https://help.openai.com/en/articles/20001530).

## Cloud apps

Apps that run entirely in the cloud have no shell on your machine and cannot start `stetdoc mcp` themselves. Claude
Cowork, ChatGPT on the web, grok.com and a dot without a connected computer can use stetdoc only through a remote MCP
server that you host and add as a custom connector, where the app allows one. stetdoc ships no hosted service.
[Self-hosting the MCP server](self-hosting-mcp.md) shows how to run one.

Meta's Muse has no route for third-party tools today.

## Building a product on stetdoc

To call stetdoc from your own code rather than through an agent, see [Build on stetdoc](build-on-stetdoc.md).
