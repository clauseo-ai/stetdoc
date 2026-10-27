# Evals of the Claude Code plugin

`evals/` is a suite that runs Claude Code with the stetdoc plugin on real legal documents and scores what it does. It
runs on Claude Code's own `claude plugin eval` (Claude Code 2.1.311 or later), so a case is a prompt a user would
type, and graders check the result. It is also the benchmark for the plugin: each case records turns, tool calls,
tokens, cost and wall time per run, so a change to the CLI, the skill or the MCP server can be measured for speed and
accuracy as well as correctness.

Every run is a real model call on your account. The suite never runs in CI or in `cargo xtask check`.

## Running it

```
evals/run.sh                                  every case, 3 runs each, then the case without stetdoc on PATH
evals/run.sh --case redline-net60-new-york --runs 1 --skip-without-stetdoc
evals/run.sh --arm none                       the same cases with no plugin loaded (stetdoc stays on PATH)
evals/run.sh --arm ~/my-docx-plugin           the same cases with another plugin, for comparison
evals/run.sh --max-cost-usd 15 -j 4           a cost ceiling, and four runs at once
evals/run.sh -- --keep-temp                   anything after -- goes to claude plugin eval
```

`run.sh` fetches the test documents into `evals/fixtures/` (see below), points `evals/arm` at the plugin under
test, runs `claude plugin eval` twice and prints a summary. The second run is the case `not-installed`, with every
directory that holds a `stetdoc` command taken out of `PATH`. The defaults are `--model claude-opus-5-5` and
`--judge-model claude-sonnet-5-5`. Pin both when you compare runs over time.

It passes `claude plugin eval` the options the suite needs: `--scaffold` (the cases' setup scripts, which you are
trusting as code from this repository), `--trust-plugin`, `--allow-real-servers` and `--allow-tools Bash Edit Write
"mcp__plugin_stetdoc_stetdoc__*"`, so the agent has the plugin's real MCP server and the CLI, as a user does. Shell
commands run in Claude Code's sandbox, which needs `bubblewrap` and `socat` on Linux and is missing on native Windows.

Each run has a temporary home, so the agent's settings, skills and memory do not leak in. If your Claude Code uses
Vertex AI, `run.sh` exports `GOOGLE_APPLICATION_CREDENTIALS` to your application default credentials, because the
temporary home hides them.

### Cost and time

A full run (8 cases and `not-installed`, 3 runs each, 27 agent runs, one arm) costs about $10 and takes about
30 minutes with the default concurrency of 1, or under 10 minutes with `-j 4`. One run of a case costs $0.10 to
$0.60: `redline-net60-new-york` was $0.39 in 34 s, `stale-colleague-edit` $0.21 in 31 s. The `llm` graders add a few
cents. Each `--arm` doubles the bill.

## Reading the results

Results go to `evals/results/<time>-<arm>/` (ignored by git):

- `summary.md`: one line per case with the score, the runs that passed, and the mean turns, tool calls, tool errors,
  input tokens (with the cached part), output tokens, cost and wall time per run, and the graders that failed in how
  many runs.
- `cases/report.html` and `without-stetdoc/report.html`: Claude Code's report, with every grader's verdict and
  explanation per run, and the judge's votes for `llm` graders.
- `cases/aggregate-result.json`: the same as JSON (`schemaVersion: 1`), for scripts.
- `metrics.jsonl`: one line per run with the tool calls by tool name, tool errors, the skills used and the tokens.

A run's score is the weighted share of its graders that passed, and a case's score is the mean over its runs. A case
passes at 1.0. Speed graders (`few-shell-calls`) and hygiene graders (no tool errors, no working copy or stray files
left) weigh half. `claude plugin eval` exits 1 when a case scores below 1.0, and `run.sh` exits with that status after
printing the summary.

## The cases

| Case | Document | What it checks |
|---|---|---|
| `read-conditions-precedent` | credit agreement | The answer cites Sections 4.01 and 4.02 (judged), and nothing is written. |
| `redline-net60-new-york` | supply agreement | Two tracked changes by the configured author in 11.1 and 23.1, a new file, nothing else changed. |
| `comment-liability-cap` | supply agreement | One comment in Clause 18 asking for 2x annual fees, no tracked change, the original untouched. |
| `accept-reject-ambiguous` | ISDA confirmation v2 | "Accept all except amounts or dates": the agent states its reading or asks (judged); any file it writes resolves all 47 revisions. |
| `compare-v1-v2` | ISDA confirmation v1 and v2 | A redline with the revised text, at least 20 revisions, one author. |
| `rename-seller-vendor` | supply agreement | All 89 "Seller" become "Vendor" as tracked changes, and nothing else changes. |
| `chained-edits` | supply agreement | Three edits into one redline, the last rejecting the first; no stale-base refusal. |
| `stale-colleague-edit` | supply agreement | A second turn (a recorded first turn) on a redline that a colleague edited in between: the colleague's change is kept and reported (judged). |
| `not-installed` | supply agreement | stetdoc is not on PATH: the agent says so and stops (judged), and never opens the package another way. |

Every case also checks that the originals are byte-identical unless the task asked otherwise, that the agent never
used `unzip`, a scripting language or the XML (`no-xml-tools`, `no-xml-reads`), and that no tool call failed.

## How grading works

`claude plugin eval` has no custom-code graders: a grader is a regex over the reply, the transcript or a file in the
workspace, a count of tool calls, or a judge model. A `.docx` is a zip, so a regex cannot read it. The suite therefore
loads a second plugin with every case, `evals/harness/`, whose only part is a `Stop` hook. When the agent stops,
`evals/harness/report.sh` writes `.stetdoc-eval-report.txt` in the workspace with:

- `unchanged:`, `changed:`, `deleted:` and `created:` lines, against SHA-256 sums the setup script recorded;
- `leftover-working-copy:` for any `NAME.stet/` folder;
- for every `.docx` the agent created or changed: `valid: yes` when `stetdoc check` passes with no more violations
  than the inputs had, `remaining-revisions: N`, the full revision ledger (`stetdoc view --revisions --full --all`),
  the comments and the text read as final.

The graders are regexes over that file, so the verdict on a document comes from stetdoc reading it, not from what the
agent says. For example, `redline-net60-new-york` asserts a deletion of thirty and an insertion of sixty by `ER`
(Eval Reviewer, the author the setup saves the way `stetdoc init --author` does) in `p145`, and no revision outside
`p145`, `p268` and `p273`. Judge graders (`llm`) are used only for the quality of an answer. The hook also reads the
session transcript and records the metrics in the report and in `metrics.jsonl`.

## The format, in short

- A case is a directory under the eval directory with `prompt.md` (front matter: `description`, `tags`, `plugins`,
  `runs`, `model`, `max_turns`, `timeout_seconds`, `allowed_tools`, `env` with `EVAL_*` keys only; the body is the
  prompt) and an optional `case.yaml` (`schema_version: "1.1"`, `name`, and `context.scaffold_script`,
  `context.history_file`, `context.add_dirs`).
- Graders are `graders/*.md`, one per file: front matter `type` (`regex`, `tool_used`, `tool_order`, `file_exists`,
  `llm`, `baseline`), `weight`, `arm`, and the type's options. A `regex` grader takes a `target`: `last_message`,
  `trace`, `files` or `{ source: file, path: ... }`.
- A run starts in an empty workspace under a temporary home. Only read-only tools are granted unless the command line
  grants more; the setup script runs only with `--scaffold`; a plugin's MCP servers start only with
  `--allow-real-servers`.
- A plugin a case loads must not sit inside the eval directory, which is why the cases are in `evals/cases/` and
  `evals/without-stetdoc/` and the plugins in `evals/arm` and `evals/harness/`.
- `--max-cost-usd 0` loads and checks every case without running one, which is a free way to validate the suite:
  `claude plugin eval . --eval-dir evals/cases --scaffold --trust-plugin --ablation none --max-cost-usd 0`.

## Arms, and what the format cannot express

`claude plugin eval` has a built-in no-plugin baseline (`--ablation with-without`), but that arm drops every plugin,
the harness too, so it cannot grade the documents. `run.sh` passes `--ablation none` and compares arms by pointing
`evals/arm` at another plugin instead, with the harness still loaded. `--arm none` is the baseline. To compare with
Anthropic's docx skill, make a plugin directory around it and pass that:

```
mkdir -p ~/docx-arm/.claude-plugin ~/docx-arm/skills
echo '{"name": "docx-arm", "version": "0.0.0"}' > ~/docx-arm/.claude-plugin/plugin.json
ln -s /path/to/anthropics/skills/skills/docx ~/docx-arm/skills/docx
evals/run.sh --arm ~/docx-arm
```

That skill uses Python packages that the sandbox must find on `PATH`, and the `no-xml-tools` grader fails it by
design, since it works on the unpacked XML.

Other limits:

- There is no way to exclude a case from a run, so `not-installed` lives in its own eval directory and `run.sh` runs
  it in a second invocation with a different `PATH`. Run it through `run.sh`: its `precondition` grader fails when
  stetdoc was on `PATH`.
- A multi-turn case can only resume a recorded conversation (`context.history_file`); nothing can change the files
  between two live turns. `stale-colleague-edit` therefore records the first turn in `history.jsonl` and makes the
  first turn's redline and the colleague's edit in its setup script.
- Tokens are not in Claude Code's result, which is why the harness reads them from the transcript.
- `no-xml-tools` sees the `Bash` tool only; an agent that read the package with another tool would get past it.
