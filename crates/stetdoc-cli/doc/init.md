Without --project, init installs into your home folder, for the agents you pick at the prompt or name with --agent, or
for the ones it detects. `stetdoc init --list` shows every agent and what is installed.

```console
$ stetdoc init --project --agent codex,claude-code
note: Claude Code can also block other ways into a .docx, such as unzip or python-docx, with the guard hook the stetdoc plugin carries. To add it to this install, see https://github.com/clauseo-ai/stetdoc/blob/main/docs/agents.md#the-claude-code-guard-hook
note: no author is saved, and every revision needs one. Run stetdoc init --author "Your Name", or set STETDOC_AUTHOR, before you edit.
wrote .agents/skills/stetdoc/SKILL.md (Codex)
wrote .claude/skills/stetdoc/SKILL.md (Claude Code)
wrote AGENTS.md

```
