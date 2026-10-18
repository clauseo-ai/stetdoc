An agent's MCP configuration starts the server; `stetdoc init --mcp` prints the line for each agent. Each tool is a command
with its flags as arguments, and answers with what the command prints with `--json`. The edit tool takes its batch in
`batch`. A session, one message per line:

```text
> {"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"unpack","arguments":{"input":"contract.docx"}}}
< {"jsonrpc":"2.0","id":1,"result":{"content":[...],"structuredContent":{"base":"...","files":[...],"working_copy":"contract.stet"},"isError":false,...}}
```

Without `--root`, the server reads and writes any file the user who runs it can, as the CLI does. With `--root`, every path
argument must stay inside that folder: a `..` or a symbolic link that leads out is refused. The check runs before each
command, so the root must be a folder only the server writes: a process that can write there could swap a link in
between the check and the command. Files of a working copy's `.stet/` that are links are refused too.

```console
$ stetdoc mcp --root nowhere
? 2
error[invalid_payload]: --root nowhere: No such file or directory (os error 2).
next: Name an existing folder with --root.

```
