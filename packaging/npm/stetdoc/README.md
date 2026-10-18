# stetdoc

The `stetdoc` command-line tool, for agents and people. It turns a `.docx` into a working copy an agent can edit with its
ordinary tools, and turns the edits back into the same file with real Word tracked changes.

```
npm install -g stetdoc           # or run it once with: npx stetdoc --help
stetdoc unpack contract.docx
stetdoc mcp                      # the MCP server over stdio; npx stetdoc mcp runs it without installing
```

The package holds no code of its own. It depends on one `@stetdoc/cli-<platform>` package, which holds the binary for your
machine, and a small launcher runs that binary with your arguments. It runs no install script.

The full documentation is in the repository: https://github.com/clauseo-ai/stetdoc. Licence: MIT or Apache-2.0.
