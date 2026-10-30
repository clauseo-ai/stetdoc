# Packaging and listings

This folder holds everything that gets stetdoc onto other people's machines and into agent catalogues. Nothing here has
been published. Each channel below says what the release fills in, and the command or page that publishes it once the
repository has a GitHub remote. A maintainer runs those steps. The release workflow only builds files.

## Placeholders and the version

- The repository is `clauseo-ai/stetdoc` on GitHub. That name appears in `install.sh` (the `REPO` line), the Homebrew
  formula, the npm packages, `packaging/pypi/pyproject.toml`, the plugin manifests under `plugins/` and `mcp-registry/server.json`. A search for
  `clauseo-ai/stetdoc` finds them all, if it ever moves.
- The version lives in `[workspace.package]` in the root `Cargo.toml`. Every other version string is a copy, except
  the Python distribution's, which maturin reads from the crate at build time (a `1.0.0-dev` workspace builds
  `1.0.0.dev0` wheels, the PEP 440 form).
  `node packaging/scripts/stamp.mjs --check` lists the copies that disagree, and `--write` fixes them. It also
  fails if `packaging/pypi/pyproject.toml` declares a static version. The release
  workflow runs `--write` before it packs anything. Before tagging a release, also run `--write` and commit, so the
  channels that install from git (the plugins) carry the same version.
- A release candidate is the tag `v<version>-rc.N` on a commit whose workspace version is `<version>`, such as
  `v0.9.0-rc.1` for 0.9.0. Every job of `release.yml` that builds or packs first runs `packaging/scripts/prerelease.mjs`,
  which stamps `<version>-rc.N` into its checkout (Cargo.toml, Cargo.lock, the npm packages and the listings), so the
  candidate's files say `0.9.0-rc.1` (npm) and `0.9.0rc1` (PyPI) and can never pass for 0.9.0. Its draft release is marked
  a prerelease, and `publish-pypi` never runs for a draft or a prerelease. To try a candidate from npm, publish its
  tarballs with `npm publish <tarball> --tag next`, never under `latest`. Any other `v*` tag that is not `v<version>` fails
  the build. Candidates are numbered on: a pushed tag is never moved, so a fix gets the next `rc.N`.

## Where the files must sit

Most files sit under `packaging/`. Each plugin has its own folder under `plugins/`, so installing one plugin copies only
that folder. Two marketplace indexes must sit at the repository root, because their docs say so. The only other packaging file at the root is `favicon.svg`, a copy of `packaging/brand/favicon.svg`, where tools look for a repository's icon.

| Path | Read by | Why it sits there |
| --- | --- | --- |
| `.claude-plugin/marketplace.json` | Claude Code | The marketplace is the repository root. Its plugin entry points at `./plugins/claude` ([marketplace reference](https://code.claude.com/docs/en/plugins/marketplace-reference.md)). |
| `.agents/plugins/marketplace.json` | Codex, ChatGPT desktop | The repository marketplace is read from `$REPO_ROOT/.agents/plugins/`, and its plugins live in `./plugins/<name>` ([OpenAI plugins](https://developers.openai.com/plugins/build/plugins)). |
| `plugins/claude/` | Claude Code | `.claude-plugin/plugin.json`, `.claude-plugin/icon.png` (the listing icon, rendered from `packaging/brand/`), `hooks/hooks.json` with `hooks/docx-guard.sh`, and `skills/stetdoc/SKILL.md`. No MCP entry: in Claude Code the skill and the `stetdoc` command are the route. |
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
- Publish: create a tap repository named `homebrew-tap`, copy the released `stetdoc.rb` to `Formula/stetdoc.rb`, and
  push it. Users then run `brew install clauseo-ai/tap/stetdoc`. Nothing in this repository pushes to a tap.

### 3. npm: the CLI

- Files: `packaging/npm/stetdoc/` holds the package `stetdoc` with `bin/stetdoc`, a Node launcher that runs the binary of
  the platform (found by `lib/platform.js`), and `lib/install.js`, a `postinstall` step that replaces the launcher with the
  binary itself, as esbuild does. The step copies a file npm already installed, downloads nothing, writes only inside the
  package and never fails the install. With `--ignore-scripts`, a package manager that blocks install scripts (pnpm 10,
  Bun, and npm 11.20 already warns that `allowScripts` will gate them), on Windows, or when the binary does not print
  `stetdoc <version>`, the launcher stays and works the same, more slowly (see "Launch cost" below). The seven packages `packaging/npm/stetdoc/targets/<target>/` are `@clauseo/stetdoc-cli-<target>`, one per target,
  and the main package lists them as `optionalDependencies`. `scripts/pack.mjs` copies the binaries in.
- Release fills: the `npm-cli` job unpacks the `cli` archives and writes eight tarballs, `stetdoc-<version>.tgz` and
  one `clauseo-stetdoc-cli-<target>-<version>.tgz` per target. The napi addon's tarballs are `clauseo-stetdoc-*.tgz`
  without `cli`, so none of them collide in the release.
- Publish: publish the seven target packages first, then the main one, from the draft's tarballs, with
  `npm publish <tarball> --access public`. Then `npx stetdoc mcp` works anywhere.
- Bun: Bun blocks the install step of any package not in `trustedDependencies`, so a plain `bun add stetdoc` or
  `bun add -g stetdoc` keeps the Node launcher (`Blocked 1 postinstall`). Bun users should install with `--trust`:
  `bun add -g --trust stetdoc` (or `bun add --trust stetdoc` in a project, which adds `stetdoc` to the project's
  `trustedDependencies`), or run `bun pm -g trust stetdoc` (`bun pm trust stetdoc` in a project) after a plain install.
  The step then runs and the binary replaces the launcher, also on a machine without Node, where Bun runs the step itself.
  For a one-off run, `bunx --bun stetdoc` starts the launcher under Bun; plain `bunx stetdoc` follows its
  `#!/usr/bin/env node` line and starts Node when Node is installed.

  Measured on 2026-10-27 with Bun 1.4.3 from the locally packed tarballs, `stetdoc --version`, median of 20 (the binary
  alone took 2.5 ms in the same run):

  | Bun install and call | Swap ran | Launcher runs under | Median |
  | --- | --- | --- | --- |
  | `bun add`, then `node_modules/.bin/stetdoc` | no (blocked) | Node | 66 ms |
  | `bun add`, then `bunx stetdoc` | no | Node (Bun when Node is missing) | 67 ms |
  | `bun add`, then `bunx --bun stetdoc` | no | Bun | 10 ms |
  | `bun add --trust` or `bun pm trust stetdoc`, then `.bin/stetdoc` | yes | none, the binary | 1.8 ms |
  | `bun add --trust`, then `bunx stetdoc` | yes | none | 4.1 ms |
  | `bun add -g`, then `stetdoc` | no (blocked) | Node | 65 ms |
  | `bun add -g --trust` or `bun pm -g trust stetdoc`, then `stetdoc` | yes | none | 1.8 ms |

  Not checked: `bunx stetdoc` straight from the registry (nothing is published), and Windows. Getting `stetdoc` onto Bun's
  built-in list of trusted packages would make the plain install fast too; that is a request to the Bun project.

### 4. PyPI: the CLI

- Files: `packaging/pypi/` holds `pyproject.toml`, the package's `README.md` (its PyPI page) and symbolic links to the
  three license files. maturin builds the `stetdoc` binary of `crates/stetdoc-cli` into a wheel with `bindings = "bin"`,
  as ruff does, so the wheel holds the native binary and nothing runs Python after the install. The pyproject sits here
  rather than at the crate or the root because `manifest-path` lets it point at the crate, which keeps the root free of
  Python files (see above) and the crate free of packaging. maturin needs the license files inside the pyproject's folder,
  hence the links. The sdist carries the CLI's crates and a trimmed workspace, so `pip` can build it with a Rust toolchain
  where no wheel fits.
- Release fills: the `pypi` job builds one wheel per target with `PyO3/maturin-action` (the Linux wheels in its
  manylinux and musllinux containers), installs the wheels a runner can run and runs `stetdoc --version`, and
  `pypi-sdist` builds the sdist. All of them go into the draft release, named `stetdoc-<pep440 version>-*.whl` and
  `stetdoc-<pep440 version>.tar.gz`. The version comes from the workspace, as above.
- Publish: publishing the draft release on GitHub starts the `publish-pypi` job, which downloads that release's seven
  wheels and the sdist and uploads them with `pypa/gh-action-pypi-publish` by trusted publishing (no token in the
  repository). Before the first release, a PyPI account with 2FA adds a pending publisher at
  https://pypi.org/manage/account/publishing/ with these fields:

  | Field | Value |
  | --- | --- |
  | PyPI project name | `stetdoc` |
  | Owner | `clauseo-ai` |
  | Repository name | `stetdoc` |
  | Workflow name | `release.yml` |
  | Environment name | `pypi` |

  and the repository gets a GitHub environment named `pypi` (Settings, Environments), ideally with a required reviewer
  and limited to `v*` tags. The first upload creates the project and turns the pending publisher into a normal one.
  Without trusted publishing, the token route: download the wheels and the sdist from the release into an empty folder
  and run `uv publish <folder>/*` (or `twine upload <folder>/*`) with an API token. The first upload needs a token scoped
  to the whole account, because a project token exists only once the project does; replace it with a project token after.
- Install: `uv tool install stetdoc`, `pipx install stetdoc` or `pip install stetdoc`. `uvx stetdoc mcp` runs the
  server without installing.
- Local build: `cd packaging/pypi && maturin build --release --locked` writes a wheel for this machine to `target/wheels`
  at the repository root.

### 5. crates.io: the engine and the CLI

- Files: none of their own. Two crates are published: `stetdoc-core` (`crates/stetdoc-core`, the engine as a library) and
  `stetdoc` (`crates/stetdoc-cli`, the binary). Both take the version, license and repository from the workspace and the
  repository README as their crates.io page. Every other crate has `publish = false`, and neither published crate needs
  one: `stetdoc-testkit` is only a dev-dependency, which cargo drops from the published manifest.
- Release fills: nothing. crates.io builds nothing; `cargo install` compiles on the user's machine with the `release`
  profile, not `dist`, and the same mimalloc allocator.
- Publish: from a clean checkout of the `v<version>` tag, after the GitHub release, run
  `cargo publish -p stetdoc-core -p stetdoc`. Cargo 1.90 or newer publishes both in dependency order and waits for
  `stetdoc-core` to reach the index before `stetdoc`. `cargo publish --dry-run -p stetdoc-core -p stetdoc` checks the
  same without uploading; it passed on 2026-10-30 at 0.9.0. A published version can be yanked but never replaced or
  deleted, and its README is frozen with it.
- Account, once: sign in at https://crates.io with GitHub, verify the email address under Account Settings, create an
  API token there with the `publish-new` and `publish-update` scopes (it is shown once), and run `cargo login`, which
  reads the token from stdin and keeps it in `~/.cargo/credentials.toml`. crates.io has no organisations: the crates
  belong to the account that publishes first. Add co-owners after with `cargo owner --add <github-login>`, or a GitHub
  team with `cargo owner --add github:clauseo-ai:<team>`.
- Later releases: once the crates exist, trusted publishing (crate Settings, Trusted Publishing, repository
  `clauseo-ai/stetdoc`, workflow `release.yml`) lets a release job publish with `rust-lang/crates-io-auth-action` and no
  stored token. No such job exists yet; the first publish has to be by hand, because a trusted publisher can only be
  added to a crate that exists.
- Install: `cargo install stetdoc --locked`, or `cargo binstall stetdoc` (not checked), which looks for a prebuilt binary.

### 6. Claude Code: plugin and marketplace

- Files: `.claude-plugin/marketplace.json` at the root, and `plugins/claude/` with `.claude-plugin/plugin.json`, the
  guard hook (`hooks/hooks.json` and `hooks/docx-guard.sh`) and the skill copy. The skill runs `stetdoc`, so the binary must
  be on the PATH (from channel 1, 2, 3, 4 or 5). The hook needs only `sh` and `awk`.
- Release fills: nothing. The version comes from the stamp script.
- Check: `claude plugin validate .` (the marketplace) and `claude plugin validate plugins/claude` (the plugin) both pass.
  The validator does not check that a marketplace's source folder exists, so `cargo xtask release check` does.
- Publish: none beyond the push. Users run `claude plugin marketplace add clauseo-ai/stetdoc`, then
  `claude plugin install stetdoc@stetdoc`. Anthropic's directory is a separate, paid submission at claude.ai/directory/manage.
- Sources: https://code.claude.com/docs/en/plugins, https://code.claude.com/docs/en/plugins-reference.md,
  https://code.claude.com/docs/en/plugins/marketplace-reference.md, https://code.claude.com/docs/en/plugins/publish.md,
  https://code.claude.com/docs/en/plugin-marketplaces

### 7. Cursor: plugin

- Files: `plugins/cursor/` with `.cursor-plugin/plugin.json` (no `$schema`, because Cursor's schema rejects unknown top-level
  keys), `mcp.json` and the skill copy.
- Publish: push the public repository, then submit it at https://cursor.com/marketplace/publish. Cursor reviews each plugin
  by hand. Local test: copy `plugins/cursor/` to `~/.cursor/plugins/local/stetdoc/` and reload the window.
- Sources: https://cursor.com/docs/reference/plugins, https://raw.githubusercontent.com/cursor/plugins/main/schemas/plugin.schema.json

### 8. OpenAI: Codex and ChatGPT

- Files: `plugins/openai/` with `plugin.json` (portable format, with its `$schema`), `mcp.json` and the skill copy, and
  `.agents/plugins/marketplace.json` at the root, which points at `./plugins/openai`.
- Install from the repository, for local Codex use: `codex plugin marketplace add clauseo-ai/stetdoc`, then install the plugin from
  the ChatGPT desktop app's Plugins Directory. The docs name no CLI command that installs one plugin.
- Out of scope: the OpenAI public directory needs a hosted HTTPS MCP server with domain verification. stetdoc does not run
  one, so this channel is for local install only. The repository's docs cover self-hosting, and nothing here submits to the
  directory.
- Sources: https://developers.openai.com/plugins/build/plugins, https://developers.openai.com/plugins/deploy/submission.md,
  https://developers.openai.com/codex/skills

### 9. MCP Registry

- Files: `mcp-registry/server.json` (the schema is `https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json`).
  Its package is `stetdoc` from npm, run with `npx stetdoc mcp` over stdio. `packaging/npm/stetdoc/package.json` carries
  `"mcpName": "io.github.clauseo-ai/stetdoc"`, which the registry checks against the server name.
- Release fills: nothing. The version comes from the stamp script.
- Publish, after channel 3 is on npm: `brew install mcp-publisher`, then from `packaging/mcp-registry/`:
  `mcp-publisher login github` (a device code in the browser, nothing stored in the repository), then `mcp-publisher publish`.
  The GitHub account must be a member of the `clauseo-ai` organisation. Check with `curl "https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.clauseo-ai/stetdoc"`.
- Sources: https://modelcontextprotocol.io/registry/quickstart, https://modelcontextprotocol.io/registry/package-types,
  https://modelcontextprotocol.io/registry/faq.md

### 10. OpenClaw ClawHub: skill

- No manifest. ClawHub takes the skill folder as it is: `skills/stetdoc/SKILL.md` is the whole skill.
- Publish: `clawhub login`, then from the repository root:
  `clawhub skill publish skills/stetdoc --slug stetdoc --name "stetdoc" --owner <owner> --categories productivity,development --topics "docx,word,tracked-changes"`.
  Omit `--owner` to publish as yourself. The `--dry-run` form for skills is unverified.
- Sources: https://docs.openclaw.ai/clawhub/publishing, https://docs.openclaw.ai/clawhub/skill-format

### 11. Hermes Skills Hub: skill

- No manifest. Hermes takes a folder with `SKILL.md`, and the frontmatter of `skills/stetdoc/SKILL.md` already fits.
- Publish to a GitHub repository, from the repository root:
  `hermes skills publish skills/stetdoc --to github --repo clauseo-ai/stetdoc`. Users then run
  `hermes skills tap add clauseo-ai/stetdoc` and `hermes skills install clauseo-ai/stetdoc/skills/stetdoc`.
- Unverified: whether the public Skills Hub takes a listing request, beyond a GitHub repository or tap.
- Sources: https://hermes-agent.nousresearch.com/docs/user-guide/features/skills,
  https://hermes-agent.nousresearch.com/docs/developer-guide/creating-skills

### 12. Agent Skills standard

`skills/stetdoc/SKILL.md` follows https://agentskills.io/specification: `name` is `stetdoc`, the same as its directory,
and `description` is under 1024 characters. The `skills-ref validate` tool named in that spec was not run.

## Launch cost and which channel to recommend

An agent may call `stetdoc` dozens of times in one task, so the cost of starting it matters. The median of 20 runs of
`stetdoc --version` on an M-series Mac mini (macOS, Node 26, npm 11.20, uv 0.13, loaded by other builds, so the numbers
are rough; detail in the packaging report):

| How it starts | Median |
| --- | --- |
| The binary (Homebrew, install script, `uv tool install`, `pipx`) | 4.8 ms |
| A pip venv's `bin/stetdoc` (the wheel's binary) | 5.5 ms |
| `npm install -g` with the install step (the launcher replaced by the binary) | 5.9 ms |
| `uvx --from <wheel> stetdoc` | 29 ms |
| `npm install -g` with `--ignore-scripts` (the Node launcher) | 106 ms |
| `npx stetdoc`, with the install step | 135 ms |
| `npx stetdoc`, with the Node launcher | 278 ms |

So recommend a binary on the PATH (Homebrew, the install script, `uv tool install stetdoc` or `pipx install stetdoc`)
to agents that run the command many times. `npx stetdoc mcp` is right for the MCP server: it starts once per session, and
npx needs no install step. `npx stetdoc` for each command costs a tenth of a second or more per call.

The shipped binaries are built with the `dist` profile of the root `Cargo.toml` (fat LTO, one codegen unit, stripped),
and the CLI allocates through mimalloc. Against the default release profile with the system allocator, on the same
machine: the binary is 7.4 MB instead of 9.9 MB; the engine benchmarks (`cargo xtask bench --filter document_ --input
contract-300p`) ran 8% faster as a geometric mean, every one of them faster; and the CLI over the engine-bench documents
(view, and replace with 1 and 20 pairs) ran about 2% faster from the profile and about 7% more from mimalloc. A rebuild
of the core and the CLI takes 68 s instead of 18 s, which is why `dist` is opt-in and everyday `--release` builds keep
the default. Unwinding stays on, because `stetdoc mcp` and the napi addon catch panics.

## Decisions and open questions

1. **npm names (decided).** Everything but the CLI lives under the Clauseo organisation's scope. The CLI is the unscoped
   `stetdoc`, so `npx stetdoc` works, with platform packages `@clauseo/stetdoc-cli-<target>`. The napi library is
   `@clauseo/stetdoc`, with platform packages `@clauseo/stetdoc-<target>`, and stays `"private": true` until the maintainers
   decide to publish it. On 2026-10-27 the registry returned 404 for `stetdoc`, `@clauseo/stetdoc`,
   `@clauseo/stetdoc-cli-darwin-arm64` and `@clauseo/stetdoc-darwin-arm64`, and "Scope not found" for
   `https://registry.npmjs.org/-/org/clauseo/package`, so a maintainer must create the npm organisation `clauseo` before
   any `@clauseo/*` package can be published. Check again before publishing, because a 404 is not a reservation. The
   Homebrew tap is `clauseo-ai/homebrew-tap` (404 on GitHub the same day), so it can hold other Clauseo formulae later.
2. **The Claude plugin** is `plugins/claude/` and holds only the skill, the manifest, the icon and the guard hook, not the repository.
3. **The author** is `Clauseo` in every manifest, as in the napi package and the Cargo manifests.

## What was verified

- Every JSON file parses with `python3 -m json.tool`. `claude plugin validate . --strict` (the marketplace) and
  `claude plugin validate plugins/claude --strict` (the plugin) both pass. The other manifests have no validator in their docs,
  so they were checked against the docs by reading only.
- `cargo xtask release check` passes, and its tests show that a stale plugin copy of the skill is named with its fix.
- The napi library's `cargo test -p stetdoc-napi` passes, and `test/smoke.mts` passes under Node 26 (61 checks) and Bun 1.3.14.
  A load through `@clauseo/stetdoc` with a `@clauseo/stetdoc-darwin-arm64` package holding the addon also works.
- `packaging/install.sh` passes `sh -n` and `shellcheck`. It was run end to end against a local mirror (a real archive and
  its checksum: the install works, a wrong checksum stops it before anything is written, and a missing release fails).
- `packaging/npm/stetdoc`: `scripts/pack.mjs` packed eight tarballs from fake binaries, and the launcher ran the platform
  binary and passed its arguments and exit status through.
- `packaging/scripts/fill-formula.mjs` filled the template from four checksum files.
- `release.yml` passes `actionlint`. The unpack step of `npm-cli` was run locally against fake archives, Windows zip included.

- PyPI (on macOS arm64): `maturin build --profile dist --locked` built the wheel, `uvx --from <wheel> stetdoc --version`
  and `stetdoc skills get core` ran through it, `pip install <wheel>` into a fresh venv put a working `stetdoc` in its
  `bin`, and the sdist built a working wheel with `uv build --wheel`.
- npm (on macOS arm64): `pack.mjs` packed the tarballs from the `dist` binary, and `npm install` of the main and the
  darwin-arm64 tarballs into a scratch project worked both ways: by default `bin/stetdoc` became the native binary, and with
  `--ignore-scripts` the launcher stayed. In both, `npx stetdoc --version` printed the version and `npx stetdoc mcp`
  answered `initialize` and `tools/list` with the twelve tools.
- The `dist` napi addon passed `test/smoke.mts` (66 checks) under Node 26.
- After the move to `@clauseo` (2026-10-27): the CLI tarballs (`stetdoc-1.0.0-dev.tgz` and
  `clauseo-stetdoc-cli-darwin-arm64-1.0.0-dev.tgz`) installed with npm with and without scripts as above, and
  `npx stetdoc mcp` listed 12 tools. The napi tarballs `clauseo-stetdoc-1.0.0-dev.tgz` and
  `clauseo-stetdoc-darwin-arm64-1.0.0-dev.tgz` installed with npm, and `require("@clauseo/stetdoc")` under Node 26 and
  `import { Document } from "@clauseo/stetdoc"` under Bun 1.4.3 loaded the addon from the platform package. The Bun runs are
  in the npm section. `publish-pypi` passes `actionlint` and has not run.
- The filled Homebrew formula passed `ruby -c`. `brew audit` takes only a tapped formula, and no tap was made.

Not verified: a real release run, the `pypi` job and maturin-action's containers, a real tap or npm install from the registry, the Homebrew `on_*` blocks on macOS, the
MCP registry schema check (no validator was available), whether `mcp-publisher` reads `server.json` from the current
directory, and the Hermes and OpenClaw listing rules beyond their docs.
