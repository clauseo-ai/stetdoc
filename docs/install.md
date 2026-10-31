# Install

Every way to install stetdoc is here, with what each one costs an agent per call and how to update, remove or script
the install.

## Pick a channel

Every channel installs the same binary. Homebrew comes first, and `uv` is the pick on a machine without it.

| Channel | Command | Platforms | Notes |
| --- | --- | --- | --- |
| Homebrew | `brew install clauseo-ai/tap/stetdoc` | macOS, Linux | From the tap `clauseo-ai/homebrew-tap`. Each call starts in about 5 ms |
| uv | `uv tool install stetdoc` | macOS, Linux, Windows | The wheel holds the native binary, so no Python runs after the install. Each call starts in about 5 ms |
| pipx | `pipx install stetdoc` | macOS, Linux, Windows | The same wheel as uv. Each call starts in about 5 ms |
| npm | `npm install -g stetdoc` | macOS, Linux, Windows | npm swaps its Node launcher for the binary when install scripts run. Each call then starts in about 6 ms |
| Install script | `curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh \| sh` | macOS, Linux | Checks the SHA-256 and installs to `~/.local/bin`. Each call starts in about 5 ms |
| Cargo | `cargo install stetdoc --locked` | Any platform with a Rust toolchain | Builds from source on crates.io |

`pip install stetdoc` into a virtual environment works too, and `uvx stetdoc` runs stetdoc once without installing it.

## Why the channel matters

An agent can run `stetdoc` dozens of times in one task, so the time each call takes to start adds up. These are medians
of 20 runs of `stetdoc --version` on an M-series Mac mini:

| How it starts | Median |
| --- | --- |
| The binary, from Homebrew, the install script, `uv tool install` or `pipx` | 4.8 ms |
| A pip virtual environment's `bin/stetdoc` | 5.5 ms |
| `npm install -g` with the install step, which puts the binary in place of the launcher | 5.9 ms |
| `uvx stetdoc` | 29 ms |
| `npm install -g --ignore-scripts`, which keeps the Node launcher | 106 ms |
| `npx stetdoc`, with the install step | 135 ms |
| `npx stetdoc`, with the Node launcher | 278 ms |

So put the binary itself on the PATH. Through `npx`, every call starts Node and npm first. Keep `npx stetdoc mcp` for
the MCP server, which starts once per session.

Bun blocks install scripts by default, so `bun add -g stetdoc` keeps the Node launcher. Install with
`bun add -g --trust stetdoc` to get the binary.

## Platforms

Each release builds stetdoc for seven targets, which cover macOS on Apple silicon and Intel, Linux on x64 and arm64 for both glibc
and musl, and Windows on x64. Homebrew and the install script give Linux the static musl build, which runs on glibc and
musl alike.

### Windows

Four channels ship a Windows x64 binary:

- `uv tool install stetdoc` or `pipx install stetdoc`. PyPI has a Windows wheel that holds `stetdoc.exe`. This is the
  one to pick.
- `npm install -g stetdoc`. On Windows the install step leaves the Node launcher in place, so every call starts Node
  first.
- The zip from the [release page](https://github.com/clauseo-ai/stetdoc/releases/latest),
  `stetdoc-x86_64-pc-windows-msvc.zip`. Check it against `stetdoc-x86_64-pc-windows-msvc.zip.sha256` with
  `Get-FileHash -Algorithm SHA256`, then put `stetdoc.exe` in a folder on your PATH.
- `cargo install stetdoc --locked`, which builds it from source.

Homebrew and the install script don't run on Windows. There is no Windows build for Arm. On Windows, `stetdoc init`
saves your name under `%APPDATA%` unless `XDG_CONFIG_HOME` is set.

## Check the install

```console
$ stetdoc --version
stetdoc 0.9.0

```

Then run `stetdoc init --list`. It prints each agent stetdoc knows, whether it found that agent on this machine, the
skill folder, the stetdoc version installed there, and the author you saved. [Use stetdoc with your agent](agents.md)
covers `stetdoc init` and each agent's setup.

## Update and uninstall

| Channel | Update | Uninstall |
| --- | --- | --- |
| Homebrew | `brew upgrade stetdoc` | `brew uninstall stetdoc` |
| uv | `uv tool upgrade stetdoc` | `uv tool uninstall stetdoc` |
| pipx | `pipx upgrade stetdoc` | `pipx uninstall stetdoc` |
| npm | `npm install -g stetdoc@latest` | `npm uninstall -g stetdoc` |
| Install script | Run the script again | `rm ~/.local/bin/stetdoc` |
| Cargo | `cargo install stetdoc --locked` | `cargo uninstall stetdoc` |

After an update, run `stetdoc init` again to bring the skill stubs up to the new version. A stub sends the agent to
`stetdoc skills get core`, which always prints the skill of the binary you run, so an old stub still works.

Before you uninstall, run `stetdoc init --remove`. It deletes the skill folders stetdoc wrote and nothing else. Your
saved name stays in `stetdoc/config.toml` in your config folder until you delete that file.

## Installing without a terminal prompt

Agents and CI run the install with no one at the keyboard. Nothing in the install asks a question, and `stetdoc init`
asks only when it runs in a terminal.

```sh
brew install clauseo-ai/tap/stetdoc
stetdoc init --agent claude-code,codex --author "Jane Doe"
stetdoc init --list
```

- `--agent` names the agents to install for, or `all`. Without it and without a terminal, `init` installs for the
  agents it detects.
- `--author` saves the name. Without a terminal and without `--author`, `init` saves nothing and says how to set one.
  On a shared or hosted machine, skip the saved name and set `STETDOC_AUTHOR` or pass `--author` on each command.
- `--dry-run` lists every file `init` would write, such as `would write ~/.claude/skills/stetdoc/SKILL.md (Claude
  Code)`, and changes nothing.
- `--json` prints the result, or the error, as JSON.

To pin a release, name it in the install:

```sh
curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh | STETDOC_VERSION=v0.9.0 sh
uv tool install stetdoc==0.9.0
npm install -g stetdoc@0.9.0
cargo install stetdoc --version 0.9.0 --locked
```

The install script reads `STETDOC_VERSION`, which defaults to `latest`, and `STETDOC_INSTALL_DIR`, which defaults to
`~/.local/bin`. It stops before it writes anything when the checksum doesn't match.
