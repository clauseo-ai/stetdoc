# stetdoc

The `stetdoc` command-line tool, for agents and people. It turns a `.docx` into a working copy an agent can edit with its
ordinary tools, and turns the edits back into the same file with real Word tracked changes.

```
uvx stetdoc --help               # run it once without installing
uv tool install stetdoc          # or: pipx install stetdoc, or pip install stetdoc
stetdoc view contract.docx --find 'net 30'
stetdoc replace contract.docx 'net 30' 'net 45'
stetdoc mcp                      # the MCP server over stdio
```

The wheel holds the native `stetdoc` binary for your platform and nothing else, so no Python runs after the install and a
call costs the same as the binary from Homebrew or the release archive. Wheels exist for macOS (Apple silicon and Intel),
Linux (x86-64 and ARM64, glibc and musl) and Windows (x86-64). On another platform pip builds the source distribution,
which needs a Rust toolchain.

The full documentation is in the repository: https://github.com/clauseo-ai/stetdoc. Licence: MIT or Apache-2.0.
