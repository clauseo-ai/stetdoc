# Packaging and listings

This folder holds everything that gets stetdoc onto other people's machines and into agent catalogues. Nothing here has
been published. Each channel below says what the release fills in, and the command or page that publishes it once the
repository has a GitHub remote. A maintainer runs those steps. The release workflow only builds files.

## Placeholders and the version

- The repository is `clauseo-ai/stetdoc` on GitHub. That name appears in `install.sh` (the `REPO` line), the Homebrew
  formula, the npm packages, the plugin manifests under `plugins/` and `mcp-registry/server.json`. A search for
  `clauseo-ai/stetdoc` finds them all, if it ever moves.
- The version lives in `[workspace.package]` in the root `Cargo.toml`. Every other version string is a copy.
  `node packaging/scripts/stamp.mjs --check` lists the copies that disagree, and `--write` fixes them. The release
  workflow runs `--write` before it packs anything. Before tagging a release, also run `--write` and commit, so the
  channels that install from git (the plugins) carry the same version.

## Where the files must sit

Most files sit under `packaging/`. Each plugin has its own folder under `plugins/`, so installing one plugin copies only
that folder. Two marketplace indexes must sit at the repository root, because their docs say so. Nothing else is at the root.

| Path | Read by | Why it sits there |
| --- | --- | --- |
| `.claude-plugin/marketplace.json` | Claude Code | The marketplace is the repository root. Its plugin entry points at `./plugins/claude` ([marketplace reference](https://code.claude.com/docs/en/plugins/marketplace-reference.md)). |
| `.agents/plugins/marketplace.json` | Codex, ChatGPT desktop | The repository marketplace is read from `$REPO_ROOT/.agents/plugins/`, and its plugins live in `./plugins/<name>` ([OpenAI plugins](https://developers.openai.com/plugins/build/plugins)). |
| `plugins/claude/` | Claude Code | `.claude-plugin/plugin.json`, `.claude-plugin/icon.png` (the listing icon, a placeholder for now), `hooks/hooks.json` with `hooks/docx-guard.sh`, and `skills/stetdoc/SKILL.md`. No MCP entry: in Claude Code the skill and the `stetdoc` command are the route. |
| `plugins/cursor/` | Cursor | `.cursor-plugin/plugin.json`, `mcp.json` and `skills/stetdoc/SKILL.md`. Cursor needs no index file for a plugin to load. |
| `plugins/openai/` | Codex, ChatGPT desktop | `plugin.json` (portable format), `mcp.json` and `skills/stetdoc/SKILL.md`. |

Each plugin folder holds its own copy of `skills/stetdoc/SKILL.md`, because a plugin is installed from its folder. The
copies must match the source: `cargo xtask release check` fails when one differs, and `cp skills/stetdoc/SKILL.md
plugins/<name>/skills/stetdoc/SKILL.md` fixes it. The source is the one that `cargo xtask docs` writes.

There is no `.mcp.json` at the root or in `plugins/claude/`. Claude Code reads a project's root `.mcp.json` and would ask
every contributor to approve the stetdoc server. The Claude Code plugin carries no MCP server either: Claude Code hides MCP
tools behind its tool search, so with both an agent switched between the tools and the command, and the server is the same
binary, so it is no backup when the command is missing. The plugin carries a guard hook instead (see
[docs/agents.md](../docs/agents.md#the-claude-code-guard-hook)).

## Channels

Each row names the file, what the release job fills, and the publish step. Both the Claude plugin and the other
repository-based channels work from the repository at its default branch, so a maintainer's push is the publish step for them.

### 1. GitHub Releases and the install script

- File: `packaging/install.sh`. POSIX sh. It detects the OS and CPU, downloads `stetdoc-<triple>.tar.gz` from the release,
  checks its SHA-256 against the `.sha256` file the `cli` job writes, and installs to `$STETDOC_INSTALL_DIR` or
  `~/.local/bin`. Linux gets the static musl build. Windows is not covered.
- Release fills: nothing. The archives and their checksums come from the `cli` jobs of `release.yml`.
- Publish: push the repository to GitHub, tag `v<version>`, and publish the draft release. Then:
  `curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh | sh`

### 2. Homebrew

- File: `packaging/homebrew/stetdoc.rb`, a template with `@VERSION@` and four `@SHA256_*@` placeholders. The `homebrew`
  job of `release.yml` runs `packaging/scripts/fill-formula.mjs` and attaches the filled file to the draft as `stetdoc.rb`.
- Publish: create a tap repository named `homebrew-stetdoc`, copy the released `stetdoc.rb` to `Formula/stetdoc.rb`, and
  push it. Users then run `brew install clauseo-ai/stetdoc/stetdoc`. Nothing in this repository pushes to a tap.

### 3. npm: the CLI

- Files: `packaging/npm/stetdoc/` holds the package `stetdoc` with `bin/stetdoc.js`, a launcher that runs the binary of
  the platform. The seven packages `packaging/npm/stetdoc/targets/<target>/` are `@stetdoc/cli-<target>`, one per target,
  and the main package lists them as `optionalDependencies`. `scripts/pack.mjs` copies the binaries in.
- Release fills: the `npm-cli` job unpacks the `cli` archives and writes eight tarballs, `stetdoc-cli-<version>.tgz` and
  one `stetdoc-cli-<target>-<version>.tgz` per target. The main tarball is renamed because the napi addon's main package
  is also named `stetdoc` (see the open question below).
- Publish: publish the seven target packages first, then the main one, from the draft's tarballs, with
  `npm publish <tarball> --access public`. Then `npx stetdoc mcp` works anywhere.

### 4. Claude Code: plugin and marketplace

- Files: `.claude-plugin/marketplace.json` at the root, and `plugins/claude/` with `.claude-plugin/plugin.json`, the
  guard hook (`hooks/hooks.json` and `hooks/docx-guard.sh`) and the skill copy. The skill runs `stetdoc`, so the binary must
  be on the PATH (from channel 1, 2 or 3). The hook needs only `sh` and `awk`.
- Release fills: nothing. The version comes from the stamp script.
- Check: `claude plugin validate .` (the marketplace) and `claude plugin validate plugins/claude` (the plugin) both pass.
  The validator does not check that a marketplace's source folder exists, so `cargo xtask release check` does.
- Publish: none beyond the push. Users run `claude plugin marketplace add clauseo-ai/stetdoc`, then
  `claude plugin install stetdoc@stetdoc`. Anthropic's directory is a separate, paid submission at claude.ai/directory/manage.
- Sources: https://code.claude.com/docs/en/plugins, https://code.claude.com/docs/en/plugins-reference.md,
  https://code.claude.com/docs/en/plugins/marketplace-reference.md, https://code.claude.com/docs/en/plugins/publish.md,
  https://code.claude.com/docs/en/plugin-marketplaces

### 5. Cursor: plugin

- Files: `plugins/cursor/` with `.cursor-plugin/plugin.json` (no `$schema`, because Cursor's schema rejects unknown top-level
  keys), `mcp.json` and the skill copy.
- Publish: push the public repository, then submit it at https://cursor.com/marketplace/publish. Cursor reviews each plugin
  by hand. Local test: copy `plugins/cursor/` to `~/.cursor/plugins/local/stetdoc/` and reload the window.
- Sources: https://cursor.com/docs/reference/plugins, https://raw.githubusercontent.com/cursor/plugins/main/schemas/plugin.schema.json

### 6. OpenAI: Codex and ChatGPT

- Files: `plugins/openai/` with `plugin.json` (portable format, with its `$schema`), `mcp.json` and the skill copy, and
  `.agents/plugins/marketplace.json` at the root, which points at `./plugins/openai`.
- Install from the repository, for local Codex use: `codex plugin marketplace add clauseo-ai/stetdoc`, then install the plugin from
  the ChatGPT desktop app's Plugins Directory. The docs name no CLI command that installs one plugin.
- Out of scope: the OpenAI public directory needs a hosted HTTPS MCP server with domain verification. stetdoc does not run
  one, so this channel is for local install only. The repository's docs cover self-hosting, and nothing here submits to the
  directory.
- Sources: https://developers.openai.com/plugins/build/plugins, https://developers.openai.com/plugins/deploy/submission.md,
  https://developers.openai.com/codex/skills

### 7. MCP Registry

- Files: `mcp-registry/server.json` (the schema is `https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json`).
  Its package is `stetdoc` from npm, run with `npx stetdoc mcp` over stdio. `packaging/npm/stetdoc/package.json` carries
  `"mcpName": "io.github.clauseo-ai/stetdoc"`, which the registry checks against the server name.
- Release fills: nothing. The version comes from the stamp script.
- Publish, after channel 3 is on npm: `brew install mcp-publisher`, then from `packaging/mcp-registry/`:
  `mcp-publisher login github` (a device code in the browser, nothing stored in the repository), then `mcp-publisher publish`.
  The GitHub account must be a member of the `clauseo-ai` organisation. Check with `curl "https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.clauseo-ai/stetdoc"`.
- Sources: https://modelcontextprotocol.io/registry/quickstart, https://modelcontextprotocol.io/registry/package-types,
  https://modelcontextprotocol.io/registry/faq.md

### 8. OpenClaw ClawHub: skill

- No manifest. ClawHub takes the skill folder as it is: `skills/stetdoc/SKILL.md` is the whole skill.
- Publish: `clawhub login`, then from the repository root:
  `clawhub skill publish skills/stetdoc --slug stetdoc --name "stetdoc" --owner <owner> --categories productivity,development --topics "docx,word,tracked-changes"`.
  Omit `--owner` to publish as yourself. The `--dry-run` form for skills is unverified.
- Sources: https://docs.openclaw.ai/clawhub/publishing, https://docs.openclaw.ai/clawhub/skill-format

### 9. Hermes Skills Hub: skill

- No manifest. Hermes takes a folder with `SKILL.md`, and the frontmatter of `skills/stetdoc/SKILL.md` already fits.
- Publish to a GitHub repository, from the repository root:
  `hermes skills publish skills/stetdoc --to github --repo clauseo-ai/stetdoc`. Users then run
  `hermes skills tap add clauseo-ai/stetdoc` and `hermes skills install clauseo-ai/stetdoc/skills/stetdoc`.
- Unverified: whether the public Skills Hub takes a listing request, beyond a GitHub repository or tap.
- Sources: https://hermes-agent.nousresearch.com/docs/user-guide/features/skills,
  https://hermes-agent.nousresearch.com/docs/developer-guide/creating-skills

### 10. Agent Skills standard

`skills/stetdoc/SKILL.md` follows https://agentskills.io/specification: `name` is `stetdoc`, the same as its directory,
and `description` is under 1024 characters. The `skills-ref validate` tool named in that spec was not run.

## Decisions and open questions

1. **npm names (decided).** The CLI is `stetdoc`, so `npx stetdoc` works, with platform packages `@stetdoc/cli-<target>`.
   The napi library is `@stetdoc/node`, with platform packages `@stetdoc/node-<target>`, and stays `"private": true` until
   the maintainers decide to publish it. `npm view` returned 404 for `stetdoc`, `@stetdoc/node` and the platform names on the
   day of the change, so they looked free. The scope `@stetdoc` did not exist on npm (`npm org ls stetdoc` found no scope),
   so a maintainer must create it before any `@stetdoc/*` package can be published. Check again before publishing, because a
   404 is not a reservation.
2. **The Claude plugin** is `plugins/claude/` and holds only the skill, the manifest, the icon and the guard hook, not the repository.
3. **The author** is `Clauseo` in every manifest, as in the napi package and the Cargo manifests.

## What was verified

- Every JSON file parses with `python3 -m json.tool`. `claude plugin validate . --strict` (the marketplace) and
  `claude plugin validate plugins/claude --strict` (the plugin) both pass. The other manifests have no validator in their docs,
  so they were checked against the docs by reading only.
- `cargo xtask release check` passes, and its tests show that a stale plugin copy of the skill is named with its fix.
- The napi library's `cargo test -p stetdoc-napi` passes, and `test/smoke.mts` passes under Node 26 (61 checks) and Bun 1.3.14.
  A load through `@stetdoc/node` with a `@stetdoc/node-darwin-arm64` package holding the addon also works.
- `packaging/install.sh` passes `sh -n` and `shellcheck`. It was run end to end against a local mirror (a real archive and
  its checksum: the install works, a wrong checksum stops it before anything is written, and a missing release fails).
- `packaging/npm/stetdoc`: `scripts/pack.mjs` packed eight tarballs from fake binaries, and the launcher ran the platform
  binary and passed its arguments and exit status through.
- `packaging/scripts/fill-formula.mjs` filled the template from four checksum files.
- `release.yml` passes `actionlint`. The unpack step of `npm-cli` was run locally against fake archives, Windows zip included.

Not verified: a real release run, a real tap or npm install from the registry, the Homebrew `on_*` blocks on macOS, the
MCP registry schema check (no validator was available), whether `mcp-publisher` reads `server.json` from the current
directory, and the Hermes and OpenClaw listing rules beyond their docs.
