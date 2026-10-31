# Product

<!-- impeccable:product-schema 1 -->

Scope: the stetdoc launch page in `site/` (stetdoc.clauseo.chat). Facts come from the site brief written by Rohan (Clauseo) and passed down by the root agent; they were not re-interviewed with Rohan directly.

## Platform

web

## Users

Agent builders: developers who run or build AI coding agents (Claude Code, Codex, Cursor, OpenCode, Copilot, Amp and others) and products that put agents to work on Word documents, especially legal contracts. They arrive from a launch post, a repo link or a colleague, decide in seconds whether stetdoc is the real thing, and install it from the terminal.

## Product Purpose

stetdoc is an open-source Rust CLI, MCP server and library (crate `stetdoc-core`) that lets AI agents read and edit Word .docx files. Every edit lands as a real Word tracked change with the agent's name on it; an unedited round trip returns identical bytes. The page announces it as the way agents work in Word and gets the visitor to run `cargo install stetdoc --locked`.

## Positioning

Agents that hand-edit .docx XML break files ("Word found unreadable content"). stetdoc validates every edit against the schema and Word's own rules before writing, applies a batch completely or not at all, computes clause numbers Word never stores, refuses ambiguous or stale edits, and leaves every untouched byte as it was. No copy of Word, no network.

## Operating Context

Terminal and agent sessions; Word documents, mostly contracts (a 527-paragraph supply agreement is the reference run). Installed into agents with `stetdoc init`, served over MCP with `stetdoc mcp`, a Claude Code plugin with a hook.

## Capabilities and Constraints

- Static Next.js 16 site on Vercel (root dir `site`), bun, server components only, CSS-only motion, Lighthouse 100 targets.
- The copy is locked word for word.
- Benchmark figures come from Rohan's docx-agent-bench runs (content/bench.ts); change none of them without new runs.

## Brand Commitments

- Clauseo's design system: warm concrete ground #E5E2DC, night the same concrete unlit (#1B1A18) by `prefers-color-scheme`; depth from planes, top-edge highlight, warm-brown shadows; no glass, blur, glow, gradients, outlined boxes or hard rules.
- Anek Latin is the only family; width carries voice. Sentence case.
- Brick (#A3482F / #D07455) only on Clauseo's comma.
- The court's three colours, at full strength and only where they mean something (Rohan, third pass): green for a check that passed and for stetdoc's results, yellow for a tracked change waiting for review and for the suggested install, red sparingly for the other setups in the benchmark. Errors and refusals stay graphite. Strikes stay ink; a tracked change is never red or green.
- stetdoc's mark: the proofreader's stet, a bar over three dots (repo `favicon.svg`). Wordmark always lowercase.
- Tone: hardcore, confident, technical, terse. No caveats, no beta, no roadmap.

## Evidence on Hand

- Real CLI output from a run on a 527-paragraph supply agreement (replace, view --find, ambiguous_match refusal), quoted in the brief.
- 20 ms tracked replace on a Mac mini; ~5 ms startup.
- Round-trip law runs nightly on ~48,000 real documents and on 306 more with 50 random edit batches each, every build.
- docx-agent-bench: 639 runs across three models against four other setups; median time per task, 0 of 118 stetdoc runs wrote a file Word offered to repair, 6 of 6 long Harvey LAB markups fully correct (content/bench.ts). Invent no others.

## Product Principles

1. Show the real run; the product is the picture.
2. The document is the user's: nothing it didn't touch changes.
3. Refuse rather than guess.
