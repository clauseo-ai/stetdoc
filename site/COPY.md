# stetdoc.clauseo.chat: launch page copy

This is the single source for every word on the page. It was written with the homepage-copy skill and checked against its blacklist and the unslop rules. Every figure is real, and the comment after each one names its source. The comments are not shown on the page.

Diagnosis: situation A, a one-line ask ("the best way for agents to interact with Word, hardcore, no caveats"). The argument is built from facts in the repo, from real runs of stetdoc 0.9.0 on evals/fixtures/master-supply-agreement.docx, and from docx-agent-bench final-2.

Read only the headings and they make the whole argument. Agents break Word files, and stetdoc doesn't. It finishes first. It keeps every byte, knows the clause numbers, refuses to guess and keeps agents out of the XML. It works in every agent. Install it.

---

## Meta

- Title: stetdoc · Let it stand
- Description: stetdoc is how AI agents work in Word. Every edit lands as a tracked change with the agent's name on it, and everything the agent didn't touch comes back exactly as it was.

## Nav

stet mark + "stetdoc" · Docs · GitHub's mark (the Octicon, no word; its accessible name is "stetdoc on GitHub")

## Hero

**H1:** Let it stand.

**Sub:** stetdoc is how AI agents work in Word. Every change your agent makes lands in Word as a tracked change with its name on it, and everything it didn't touch comes back exactly as it was.

**CTA:** `brew install clauseo-ai/tap/stetdoc` with a copy button ("Copy the install command", then "Copied"). Beside it, a small label "Suggested", and a link "Other ways to install" that jumps to #install. Then Read the docs · GitHub.

**Under the CTA:** Free and open source, MIT or Apache-2.0.

**Picture:** the terminal run and the Word sheet with clause 11.1's tracked change. Use the run exactly as it is in the brief.

**Caption:** Unedited output from a 527-paragraph supply agreement. The edit took 20 milliseconds.
<!-- measured: target/release/stetdoc replace on msa.docx, 5 runs, real 0.02 s each -->

---

## 1. Word found unreadable content.

Every agent that edits a .docx's XML meets this dialog sooner or later. Word splits a sentence across many runs and stores tracked changes as XML structure. It refuses a file whose elements fall out of schema order. One slip in the XML and the document won't open.

stetdoc checks every edit against the schema and Word's own rules before it writes a byte. A batch of edits applies completely or not at all.

**Artifact:** the plain Word dialog ("Microsoft Word" / "Word found unreadable content in contract.docx. Do you want to recover the contents of this document?" / Yes, No). Beside it, stetdoc's line: `validated 1 unit: no problems, and this file needs no separate check.`

---

## 2. Done first, on every model.

We gave agents eight contract-editing tasks five ways: with stetdoc, with no plugin, with Anthropic's docx skill, with Anthropic's legal plugin and with Adeu. Then we opened every file they wrote in Word. The agents using stetdoc finished first on all three models.

**Chart: median time per task.** stetdoc is a solid bar, and the other four setups show as a range from fastest to slowest.
- Claude Haiku 5.5: stetdoc 21 s. Other setups 41 to 45 s.
- Claude Opus 5.5: stetdoc 25 s. Other setups 37 to 56 s.
- Gemini 3.8 Flash: stetdoc 135 s. Other setups 274 to 538 s.
<!-- docx-agent-bench results/final-2/tables.md, process metrics, short tasks, medians per run -->

**Three facts:**
- **0 of 118:** stetdoc runs that wrote a file Word offered to repair. <!-- runs.csv word_valid -->
- **6 of 6:** long Harvey LAB markups, fully correct on every model. <!-- tables.md long tasks -->
- **1 ms:** from launch to a ready MCP server. <!-- engine-bench.md, MCP cold start -->

**Method line:** docx-agent-bench, 639 runs in Claude Code 2.1.311 and Antigravity CLI 1.4.8. Five runs per task, medians per run, and every file opened in Microsoft Word for Mac 16.114.3.

---

## 3. Open, save, same bytes.

Run a document through stetdoc without editing it and you get the identical file back, byte for byte. Edit one clause and only that clause's elements change. Styles, numbering, comments, other people's tracked changes and macros stay where their authors left them.

**Proof line:** We test this every night on about 48,000 documents from 77 public sources. Every build also puts 306 more through 50 random edit batches each.
<!-- README.md 147-151, corpus/SOURCES.md, corpus/curated.tsv -->

**Artifact:** a short stack of paragraph planes. One paragraph lifts and takes the tracked change, and the rest stay locked and identical. Then:
```
$ stetdoc unpack contract.docx && stetdoc pack contract.stet -o roundtrip.docx
$ cmp contract.docx roundtrip.docx && echo identical
identical
```
<!-- run 2026-10-31 on msa.docx: identical -->

---

## 4. It knows where 11.1 is.

Lawyers cite clauses by number. Word works those numbers out when it draws the page, so they appear nowhere in the file. stetdoc computes them, and a search hands your agent the clause, a stable paragraph ref and the exact text.

**Artifact:** the `stetdoc view msa.docx --find 'thirty' --find 'governing law'` output from the brief.

---

## 5. Twelve matches is not one.

`replace` matches text the way it reads, across bold, italics and run breaks. When the text appears more than once, stetdoc writes nothing and lists every match with its clause. Missing text gets the same refusal. stetdoc also refuses an edit aimed at a paragraph that changed after the agent read it, so a change never lands in the wrong place.

**Artifact:** the `error[ambiguous_match]` output from the brief.

---

## 6. Hands off the XML.

stetdoc gives your agent the document as text, with a ref on every paragraph and a number on every clause. The zip and its XML stay with stetdoc. In Claude Code, the plugin adds a hook that stops Claude from unzipping a .docx, scripting it with python-docx or rewriting its XML. The only way into the file is the one stetdoc checks.
<!-- docs/agents.md 160-187: the guard blocks unzipping, scripting or python-docx on .docx files, and XML edits -->

---

## 7. One install for every agent you run.

stetdoc init puts its skill in Claude Code, Codex, Cursor, OpenCode, Copilot, Amp, Antigravity, Factory, Grok Build, Hermes, OpenClaw and cmx2. Agents that take tools instead of a shell get the same commands over MCP from stetdoc mcp. If you're building a product, the engine is also a Rust crate, stetdoc-core, and stetdoc mcp can serve it to cloud apps you host.

**Artifact:** the For agents block, headed "For agents" with its anchor `#agents-install`. It opens with the entity sentence, then the plain-text install an agent can run, with its own Copy button. Both texts live in site/content/copy.ts as `ENTITY` and `AGENT_BLOCK`, and index.md carries them in the same place. llms.txt carries the block under "Install and connect".

---

## End

**H2:** Install stetdoc.

**Intro:** Pick any channel. Every one installs the same binary.

**Picker:** a radio group labelled "Install with". Homebrew is selected first. Build it CSS-only with radio inputs and :checked, so it needs no JS. Every command has to be in the HTML even while its panel is hidden, and each panel has its own copy button.

| Channel | Command | Note |
| --- | --- | --- |
| Homebrew, labelled Suggested | `brew install clauseo-ai/tap/stetdoc` | macOS and Linux. Each call starts in about 5 ms. |
| uv | `uv tool install stetdoc` | The wheel holds the native binary, so no Python runs after the install. Each call starts in about 5 ms. |
| pipx | `pipx install stetdoc` | The same wheel as uv. Each call starts in about 5 ms. |
| npm | `npm install -g stetdoc` | npm swaps its Node launcher for the binary when install scripts run. Each call then starts in about 6 ms. |
| Install script | `curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh \| sh` | macOS and Linux. It checks the SHA-256 and installs to ~/.local/bin. Each call starts in about 5 ms. |
| Cargo | `cargo install stetdoc --locked` | Builds from source on crates.io, so it needs a Rust toolchain. |
<!-- packaging/README.md 238-256 and docs/agents.md 15-34: median of 20 runs of stetdoc --version. The binary from Homebrew, the install script, uv tool or pipx starts in 4.8 ms, npm -g with the install step in 5.9 ms, npx with the install step in 135 ms, and npx with the Node launcher in 278 ms. Cargo wasn't measured. -->

**Why Homebrew is suggested, set under the picker:** An agent can run stetdoc dozens of times in one task. Homebrew, uv, pipx and the install script put the binary itself on your PATH, so each call starts in about 5 ms. Through npx, every call starts Node and npm first, which costs a tenth of a second or more, so keep `npx stetdoc mcp` for the MCP server, which starts once per session.

**Then:** Read the docs · GitHub

**Footnote:** Stet is the proofreader's mark for "let it stand". It goes under a word that was struck out by mistake, and the word stays.

## Footer

stetdoc is open source, from [Clauseo lockup] · Docs · GitHub's mark ("stetdoc on GitHub") · Changelog · llms.txt · Sitemap

---

## Headline candidates considered

1. **Let it stand.** Chosen. A line of wit that carries a claim. Stet means "leave the original", and the claim under it is the byte-identity guarantee. It names the product's own mark, and it will still be true in ten years. The subhead's first sentence gives the plain label straight away, so the five-second test still passes.
2. **Word found unreadable content.** This names the dialog every agent builder has hit and won't say out loud. It's the strongest problem line, so it opens section 1 instead of the page.
3. **Hands off the XML.** A point of view that costs something, because the common approach has agents unzip the package and edit its XML. It became section 6.

## Gaps

- [NEED: a named quote] One sentence from a real user or design partner, with their name and role, would be the strongest proof on the page. None exists in the repo.
- [NEED: publish] Every install command only works once its channel is live. On 2026-10-31, crates.io, PyPI and npm all returned 404 for stetdoc. The Homebrew tap repository exists, but its formula wasn't checked.

## UI strings

Lines the design needs that the sections above don't give. Written to the same rules: plain words, sentence case, straight quotes, no dashes, no parentheses, no mid-sentence colons. Artifact text that quotes real stetdoc, Word or document output keeps that output's own punctuation. All of these live in site/content/copy.ts with the rest of the copy.

**Copy buttons:** visible label "Copy"; after a copy it says "Copied" for 1.5 s, and a polite live region announces "Copied". Accessible name "Copy the install command". If the browser blocks both ways of copying, the command gets selected and the live region says "Selected. Copy it with your keyboard."

**Hero Word sheet:** title strip "msa.edited.docx"; heading above the clause "Clause 11. Payment"; under the clause, the real start of clause 11.2 from the supply agreement, cut at two lines. Balloon "Claude", "Deleted: thirty (30", "31 Oct 2026, 08:24". Screen readers hear "deleted:" before the struck words and "inserted:" before the new ones.

**Dialog artifact:** group label for screen readers "Word's dialog".

**Benchmark table:** caption "Median time per task". Column headers, read by screen readers only, "Model", "stetdoc", "Other setups, fastest to slowest". Visual key "stetdoc" and "The other four setups, fastest to slowest". Each row sets the two figures head to head, each under a small label: "stetdoc" over "21 s", and "Other setups" over "41 to 45 s", "37 to 56 s" and "274 to 538 s". Inside each stetdoc bar "stetdoc". Each track runs from 0 to the row's slowest setup and is labelled at its ends, "0" and "slowest 45 s", "slowest 56 s", "slowest 538 s", so each row's scale explains itself.

**Same-bytes stack:** paragraph refs "p143" to "p147" with "§11.1" on p145. The locked paragraphs show the supply agreement's real text for p143, p144, p146 and p147, cut at two lines; the lifted one shows clause 11.1 with its tracked change.

**Bytes artifact:** the cmp output "identical" is set on its own verdict bar, the same bar as every other verdict.

**Twelve matches artifact:** the output's last line, "next: Nothing was applied, …", is set on its own bar under the well.

**Hands off the XML artifact:** the agent's attempt "$ unzip -o msa.docx -d msa", struck through, then the guard's real reply from plugins/claude/hooks/docx-guard.sh, trimmed: "stetdoc's guard blocked this: it opens a Word package with an archive tool. A Word file (.docx) is edited only through stetdoc, which records every change as a Word tracked change and leaves the rest of the file untouched. … Do not try another way around this."

**For agents block:** on a phone the block shows the entity sentence, the Copy key and the prompt's first two steps; the rest of the prompt opens under a disclosure labelled "Show all five steps". The Copy key still copies all five, and the page keeps the whole prompt in its HTML.

**Navigation labels for screen readers:** top nav "stetdoc"; footer nav "More".

**Share card (og.png):** "stetdoc", "Let it stand.", "stetdoc is how AI agents work in Word." Alt text "stetdoc. Let it stand."
