# Troubleshooting

Here are the problems people hit most often, each with the command that fixes it. Every refusal also prints a
`next:` line, and the [error reference](reference/errors.md) explains each code.

## stetdoc says no author is set

```console
$ stetdoc replace contract.docx 'thirty days' 'sixty days'
? 2
error[invalid_payload]: No author: every revision carries one, and stetdoc never picks one for you.
next: Run stetdoc init to save your name, or set STETDOC_AUTHOR, or pass --author "Your Name".

```

Every tracked change carries an author, and stetdoc never takes one from git or the system. Save your name once with
`stetdoc init --author "Your Name"`. On a shared or hosted machine, set `STETDOC_AUTHOR` or pass `--author` on each
command, because the saved name signs every user's edits. [The author](agents.md#the-author) gives the order stetdoc
reads them in.

## replace says ambiguous_match

The text you asked to replace is in the document more than once, so stetdoc wrote nothing and listed every place. Pass
`--all` to change all of them, `--range p1..p1` with a ref from the list to change one, or make the old text longer so
it matches once. `--expect 3` changes them only when there are exactly three. See
[`ambiguous_match`](reference/errors.md#ambiguous_match), and [the quickstart](quickstart.md#watch-it-refuse) for a
worked example.

## pack refuses with stale_target

```console
$ stetdoc pack contract.stet -o revised.docx --author "Jane Doe"
? 4
error[stale_target]: revised.docx exists and is neither the document contract.stet came from nor one it packed, so packing would overwrite a version it has not seen.
next: Unpack revised.docx again (or into a new folder) and redo the edits on it, pack to another path with -o, or pass --force to overwrite it.

```

The file you're writing to holds a version this working copy has never seen, often someone else's newer draft. Unpack
that file and redo the edits on it, or pack to another path with `-o`. `--force` overwrites it, so use it only when you
know that version can go. See [`stale_target`](reference/errors.md#stale_target).

## My agent doesn't see the skill

Run `stetdoc init --list`. It shows each agent, whether stetdoc detected it, its skill folder and the stetdoc version
installed there. If the installed column says `no` for your agent, name it with `stetdoc init --agent claude-code`, or whichever
agent you use, and start a new session. Agents read their skills when a session starts. Each agent's section in
[Use stetdoc with your agent](agents.md) gives the command that lists its skills.

## My agent unzipped the .docx anyway

An agent reaches for the zip when stetdoc is missing from its PATH or its skill isn't installed. Check both with
`stetdoc --version` and `stetdoc init --list`. In Claude Code, install the plugin or add the
[guard hook](agents.md#the-claude-code-guard-hook), which blocks unzip, python-docx and XML edits on a Word file. With
an MCP server, install the skill too, so an agent whose server fails to start stops instead of editing the XML.

## Word says it found unreadable content

If stetdoc wrote the file, check that nothing else touched it afterwards, such as an agent's script. stetdoc validates
every edit before it writes, so a file it wrote that Word won't open is a bug. Please
[report it](https://github.com/clauseo-ai/stetdoc/issues) with the input and the command.

If the input itself won't open, stetdoc may still read it. stetdoc repairs one cause of this itself. When the file
names a namespace prefix in `mc:Ignorable` and never declares it, stetdoc declares it on the next write, and the receipt
says so in a `repaired` warning. `stetdoc check contract.docx --all` lists the other problems the file already had.

## The MCP server won't start

Run the server by hand. It prints one line and waits for a client:

```console
$ stetdoc mcp < /dev/null
stetdoc mcp 0.9.0: 11 tools on stdin and stdout

```

If the shell says `command not found`, the agent can't find it either. [Install](install.md) stetdoc on the PATH, or use
`npx stetdoc mcp` in the agent's config. A `--root` folder must exist:

```console
$ stetdoc mcp --root ./documents < /dev/null
? 2
error[invalid_payload]: --root ./documents: No such file or directory (os error 2).
next: Name an existing folder with --root.

```

If the server runs by hand but not in the agent, compare the agent's config with its section in
[Use stetdoc with your agent](agents.md). `stetdoc init --mcp` prints the line for each agent.
