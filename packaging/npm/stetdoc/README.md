# stetdoc

The `stetdoc` command-line tool, for agents and people. It turns a `.docx` into a working copy an agent can edit with its
ordinary tools, and turns the edits back into the same file with real Word tracked changes.

```
npm install -g stetdoc           # or run it once with: npx stetdoc --help
stetdoc unpack contract.docx
stetdoc mcp                      # the MCP server over stdio; npx stetdoc mcp runs it without installing
```

The package holds no code of its own. It depends on one `@clauseo/stetdoc-cli-<platform>` package, which holds the binary
for your machine, and a small launcher runs that binary with your arguments. An install step, as esbuild has, then replaces
the launcher with that binary, so each call skips starting Node. It copies a file npm already installed, downloads nothing,
and never fails the install. With `--ignore-scripts`, and on Windows, the launcher stays and works the same, a little slower.

With Bun, install with `bun add -g --trust stetdoc` (or run `bun pm -g trust stetdoc` after a plain install), because Bun
blocks the install step of untrusted packages and the launcher then starts Node on each call. For a one-off run use
`bunx --bun stetdoc`, which starts the launcher under Bun.

An agent that runs `stetdoc` many times is fastest with a binary on the PATH: `brew install clauseo-ai/tap/stetdoc`,
`uv tool install stetdoc`, or the install script in the repository. `npx stetdoc mcp` suits the MCP server, which starts once.

The full documentation is in the repository: https://github.com/clauseo-ai/stetdoc. Licence: MIT or Apache-2.0.
