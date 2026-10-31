/**
 * Every word on stetdoc.clauseo.chat, in reading order, from site/COPY.md.
 * The HTML page (app/page.tsx) renders from here, and so can a markdown
 * version of the page (/index.md, /llms-full.txt) when it is built. No copy
 * lives in components.
 *
 * Inline marks, the same in both renderings: `code` for a command named in
 * running text, **bold** for words set in primary weight. Terminal lines
 * are real stetdoc output; `cmd` lines were typed at a shell prompt, `out`
 * lines are what came back, `raw` lines were typed at an agent's prompt.
 */

import {
	AGENT_GUIDE_URL,
	CHANGELOG_URL,
	CLAUSEO_URL,
	DOCS_URL,
	GITHUB_URL,
	INSTALL_COMMAND,
	SITE_URL,
} from '../app/links';
import { BENCH_FACTS, BENCH_METHOD, BENCH_ROWS } from './bench';

/** `passed` marks a line where stetdoc reports a check that passed; its bold
    words take court green */
export type Line = { kind: 'cmd' | 'out' | 'raw'; text: string; passed?: boolean };

export const SITE = {
	name: 'stetdoc',
	url: SITE_URL,
	title: 'stetdoc · Let it stand',
	description:
		"stetdoc is how AI agents work in Word. Every edit lands as a tracked change with the agent's name on it, and everything the agent didn't touch comes back exactly as it was.",
	install: INSTALL_COMMAND,
	licence: 'Free and open source, MIT or Apache-2.0.',
	links: {
		docs: DOCS_URL,
		github: GITHUB_URL,
		changelog: CHANGELOG_URL,
		clauseo: CLAUSEO_URL,
	},
} as const;

/** what stetdoc is, in one sentence: the For agents block, llms.txt, JSON-LD */
export const ENTITY =
	'stetdoc is an open-source CLI, MCP server and Rust library that lets AI agents read and edit Word .docx files, with every edit recorded as a Word tracked change.';

/**
 * The plain-text install an agent can run without a human. The same text is
 * on the page, in index.md and in llms.txt. MCP lines cover three agents in
 * alphabetical order, and `stetdoc init --mcp` prints the line for any other.
 */
export const AGENT_BLOCK = {
	id: 'agents-install',
	title: 'For agents',
	text: [
		'Install stetdoc and connect it to this agent. Run these yourself.',
		'1. brew install clauseo-ai/tap/stetdoc',
		`   Without Homebrew, pick another channel at ${SITE_URL}/#install`,
		'2. stetdoc --version',
		'3. stetdoc init',
		'   This puts the stetdoc skill in every agent it finds.',
		'4. To use stetdoc over MCP as well, register it with your agent:',
		'   Claude Code: claude mcp add --scope user stetdoc -- stetdoc mcp',
		'   Codex: codex mcp add stetdoc -- stetdoc mcp',
		'   Cursor: add {"mcpServers":{"stetdoc":{"command":"stetdoc","args":["mcp"]}}} to ~/.cursor/mcp.json',
		'   Any other agent: stetdoc init --mcp prints the line for it',
		`5. Read ${AGENT_GUIDE_URL} before you edit a document.`,
	].join('\n'),
	copyLabel: 'Copy the agent install steps',
	/** on a phone the block shows the first two steps; this opens the rest */
	showAll: 'Show all five steps',
};

export const NAV = {
	docs: 'Docs',
	github: 'GitHub',
	readDocs: 'Read the docs',
	suggested: 'Suggested',
	otherWays: 'Other ways to install',
};

export const HERO = {
	h1: 'Let it stand.',
	/** the word the stet dots go under */
	stetWord: 'stand',
	sub: "stetdoc is how AI agents work in Word. Every change your agent makes lands in Word as a tracked change with its name on it, and everything it didn't touch comes back exactly as it was.",
	caption:
		'Unedited output from a 527-paragraph supply agreement. The edit took 20 milliseconds.',
	run: [
		{
			kind: 'cmd',
			text: "stetdoc replace msa.docx 'due within thirty (30) days' 'due within forty-five (45) days'",
		},
		{
			kind: 'out',
			text: 'replace "due within thirty (30) days" with "due within forty-five (45) days": **once, at p145 (clause 11.1)**',
		},
		{
			kind: 'out',
			text: 'applied 1 operation, **tracked as Claude 2026-10-31T08:24:25Z**',
		},
		{
			kind: 'out',
			text: 'validated 1 unit: **no problems**, and this file needs no separate check.',
			passed: true,
		},
		{ kind: 'out', text: 'wrote **msa.edited.docx**' },
		{ kind: 'out', text: 'msa.docx is **unchanged**.' },
	] satisfies Line[],
	/** the file the run wrote, as Word shows it */
	sheet: {
		file: 'msa.edited.docx',
		heading: 'Clause 11. Payment',
		before: '11.1 Payment for each Shipment of Products shall be due within ',
		deleted: 'thirty (30',
		inserted: 'forty-five (45',
		after: ') days from the date of the Bill of Lading for the relevant Shipment (the “Due Date”). The amount payable shall be determined in accordance with the pricing provisions of Clause 5.',
		next: '11.2 Payment for each Shipment shall be made by the Buyer by means of an irrevocable documentary letter of credit, confirmed by a first-class international bank acceptable to the Seller.',
		balloon: {
			author: 'Claude',
			label: 'Deleted:',
			text: 'thirty (30',
			when: '31 Oct 2026, 08:24',
		},
	},
};

export const PARTS = {
	unreadable: {
		id: 'unreadable',
		title: 'Word found unreadable content.',
		body: [
			"Every agent that edits a .docx's XML meets this dialog sooner or later. Word splits a sentence across many runs and stores tracked changes as XML structure. It refuses a file whose elements fall out of schema order. One slip in the XML and the document won't open.",
			"stetdoc checks every edit against the schema and Word's own rules before it writes a byte. A batch of edits applies completely or not at all.",
		],
		dialog: {
			title: 'Microsoft Word',
			text: 'Word found unreadable content in contract.docx. Do you want to recover the contents of this document?',
			buttons: ['Yes', 'No'],
		},
		verdict:
			'validated 1 unit: **no problems**, and this file needs no separate check.',
	},
	bench: {
		id: 'bench',
		title: 'Done first, on every model.',
		body: [
			"We gave agents eight contract-editing tasks five ways: with stetdoc, with no plugin, with Anthropic's docx skill, with Anthropic's legal plugin and with Adeu. Then we opened every file they wrote in Word. The agents using stetdoc finished first on all three models.",
		],
		chart: {
			caption: 'Median time per task',
			columns: ['Model', 'stetdoc', 'Other setups, fastest to slowest'],
			keyStet: 'stetdoc',
			keyOthers: 'The other four setups, fastest to slowest',
			/** the label over each row's quieter figure */
			othersLabel: 'Other setups',
			rows: BENCH_ROWS,
			range: (fast: number, slow: number) => `${fast} to ${slow} s`,
			/** the ends of each row's track, which runs from 0 to its slowest setup */
			scaleStart: '0',
			scaleEnd: (slow: number) => `slowest ${slow} s`,
		},
		facts: BENCH_FACTS,
		method: BENCH_METHOD,
	},
	bytes: {
		id: 'bytes',
		title: 'Open, save, same bytes.',
		body: [
			"Run a document through stetdoc without editing it and you get the identical file back, byte for byte. Edit one clause and only that clause's elements change. Styles, numbering, comments, other people's tracked changes and macros stay where their authors left them.",
		],
		proof: 'We test this every night on **about 48,000 documents** from **77** public sources. Every build also puts **306** more through **50** random edit batches each.',
		/** the paragraphs around 11.1 in the supply agreement, as stetdoc reads them */
		stack: [
			{
				ref: 'p143',
				text: '10.2 The Buyer shall, at its own cost and expense, maintain adequate insurance coverage for the Products from the point at which risk passes under Clause 10.1 through to the point of final delivery to the Buyer’s storage facilities or end customers.',
			},
			{ ref: 'p144', text: 'CLAUSE 11 — PAYMENT', heading: true },
			{ ref: 'p145', clause: '§11.1', changed: true },
			{
				ref: 'p146',
				text: '11.2 Payment for each Shipment shall be made by the Buyer by means of an irrevocable documentary letter of credit, confirmed by a first-class international bank acceptable to the Seller.',
			},
			{
				ref: 'p147',
				text: '11.3 The Buyer shall procure the issuance of the Letter of Credit not less than ten (10) Business Days prior to the first day of the loading window specified in the accepted Purchase Order for the relevant Shipment.',
			},
		],
		run: [
			{
				kind: 'cmd',
				text: 'stetdoc unpack contract.docx && stetdoc pack contract.stet -o roundtrip.docx',
			},
			{
				kind: 'cmd',
				text: 'cmp contract.docx roundtrip.docx && echo identical',
			},
		] satisfies Line[],
		verdict: 'identical',
	},
	clauses: {
		id: 'clauses',
		title: 'It knows where 11.1 is.',
		body: [
			'Lawyers cite clauses by number. Word works those numbers out when it draws the page, so they appear nowhere in the file. stetdoc computes them, and a search hands your agent the clause, a stable paragraph ref and the exact text.',
		],
		command:
			"stetdoc view msa.docx --find 'thirty' --find 'governing law'",
		hits: [
			{
				ref: 'p145',
				clause: '§11.1',
				text: '…each Shipment of Products shall be due within thirty (30) days from the date of the Bill of Lading…',
			},
			{
				ref: 'p182',
				clause: '§14.3',
				text: "…Party may terminate this Agreement by giving thirty (30) days' written notice to the other Party.…",
			},
			{
				ref: 'p211',
				clause: '§16.4(c)',
				text: '…22 (Dispute Resolution), Clause 23 (Governing Law), Clause 24 (Entire Agreement), and Clause…',
			},
			{ ref: 'p272', clause: '', text: 'CLAUSE 23 — GOVERNING LAW' },
		],
		total: '16 matches in 15 blocks',
	},
	ambiguous: {
		id: 'ambiguous',
		title: 'Twelve matches is not one.',
		body: [
			'`replace` matches text the way it reads, across bold, italics and run breaks. When the text appears more than once, stetdoc writes nothing and lists every match with its clause. Missing text gets the same refusal. stetdoc also refuses an edit aimed at a paragraph that changed after the agent read it, so a change never lands in the wrong place.',
		],
		run: [
			{
				kind: 'cmd',
				text: "stetdoc replace msa.docx 'thirty (30) days' 'forty-five (45) days'",
			},
			{
				kind: 'out',
				text: '**error[ambiguous_match]**: "thirty (30) days" is in the document 12 times, and it must be there once:',
			},
		] satisfies Line[],
		places: [
			'p81 (clause 3.3) …d to the Seller not less than [thirty (30) days] prior to the first day of the…',
			'p126 (clause 7.5(a)) …cement to be completed within [thirty (30) days] of the date on which the non-…',
			'p145 (clause 11.1) … Products shall be due within [thirty (30) days] from the date of the Bill of …',
			"p182 (clause 14.3) …nate this Agreement by giving [thirty (30) days]' written notice to the other …",
			'…',
		],
		verdict:
			'next: **Nothing was applied**, so retry at once: to change every one, pass --all; to pick one place, pass --range with its ref, such as --range p81..p81, or make OLD longer so it matches once.',
	},
	xml: {
		id: 'xml',
		title: 'Hands off the XML.',
		body: [
			'stetdoc gives your agent the document as text, with a ref on every paragraph and a number on every clause. The zip and its XML stay with stetdoc. In Claude Code, the plugin adds a hook that stops Claude from unzipping a .docx, scripting it with python-docx or rewriting its XML. The only way into the file is the one stetdoc checks.',
		],
		attempt: 'unzip -o msa.docx -d msa',
		/** plugins/claude/hooks/docx-guard.sh's reply to that command, trimmed */
		verdict:
			"**stetdoc's guard blocked this: it opens a Word package with an archive tool.** A Word file (.docx) is edited only through stetdoc, which records every change as a Word tracked change and leaves the rest of the file untouched. … Do not try another way around this.",
	},
	agents: {
		id: 'agents',
		title: 'One install for every agent you run.',
		body: [
			"`stetdoc init` puts its skill in Claude Code, Codex, Cursor, OpenCode, Copilot, Amp, Antigravity, Factory, Grok Build, Hermes, OpenClaw and cmx2. Agents that take tools instead of a shell get the same commands over MCP from `stetdoc mcp`. If you're building a product, the engine is also a Rust crate, `stetdoc-core`, and `stetdoc mcp` can serve it to cloud apps you host.",
		],
		/* the artifact is the For agents block (AGENT_BLOCK) */
	},
} as const;

/** the parts in page order */
export const PART_ORDER = [
	'unreadable',
	'bench',
	'bytes',
	'clauses',
	'ambiguous',
	'xml',
	'agents',
] as const satisfies readonly (keyof typeof PARTS)[];

export type Channel = {
	id: string;
	name: string;
	suggested?: boolean;
	command: string;
	note: string;
};

export const END = {
	id: 'install',
	title: 'Install stetdoc.',
	intro: 'Pick any channel. Every one installs the same binary.',
	pickerLabel: 'Install with',
	channels: [
		{
			id: 'brew',
			name: 'Homebrew',
			suggested: true,
			command: INSTALL_COMMAND,
			note: 'macOS and Linux. Each call starts in about 5 ms.',
		},
		{
			id: 'uv',
			name: 'uv',
			command: 'uv tool install stetdoc',
			note: 'The wheel holds the native binary, so no Python runs after the install. Each call starts in about 5 ms.',
		},
		{
			id: 'pipx',
			name: 'pipx',
			command: 'pipx install stetdoc',
			note: 'The same wheel as uv. Each call starts in about 5 ms.',
		},
		{
			id: 'npm',
			name: 'npm',
			command: 'npm install -g stetdoc',
			note: 'npm swaps its Node launcher for the binary when install scripts run. Each call then starts in about 6 ms.',
		},
		{
			id: 'script',
			name: 'Install script',
			command:
				'curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh | sh',
			note: 'macOS and Linux. It checks the SHA-256 and installs to ~/.local/bin. Each call starts in about 5 ms.',
		},
		{
			id: 'cargo',
			name: 'Cargo',
			command: 'cargo install stetdoc --locked',
			note: 'Builds from source on crates.io, so it needs a Rust toolchain.',
		},
	] satisfies Channel[],
	why: 'An agent can run stetdoc dozens of times in one task. Homebrew, uv, pipx and the install script put the binary itself on your PATH, so each call starts in about 5 ms. Through npx, every call starts Node and npm first, which costs a tenth of a second or more, so keep `npx stetdoc mcp` for the MCP server, which starts once per session.',
	footnote:
		'Stet is the proofreader\'s mark for "let it stand". It goes under a word that was struck out by mistake, and the word stays.',
};

export const FOOTER = {
	from: 'stetdoc is open source, from',
	changelog: 'Changelog',
	/* for agents: the site's index of its markdown, and its sitemap */
	llms: 'llms.txt',
	sitemap: 'Sitemap',
};

/** strings the design needs; listed under "UI strings" in COPY.md */
export const UI = {
	copy: 'Copy',
	copied: 'Copied',
	copyLabel: 'Copy the install command',
	jsonLdDescription:
		'Open-source CLI, MCP server and Rust library that lets AI agents read and edit Word .docx files with tracked changes.',
	copySelected: 'Selected. Copy it with your keyboard.',
	deleted: 'deleted: ',
	inserted: 'inserted: ',
	dialogLabel: "Word's dialog",
	navLabel: 'stetdoc',
	footNavLabel: 'More',
	/** the accessible name of the GitHub mark in the nav and the footer */
	githubMark: 'stetdoc on GitHub',
	ogAlt: 'stetdoc. Let it stand.',
};
