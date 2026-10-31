/**
 * The docs pages: which repo file each page renders, its URL, its place in
 * the sidebar and its meta description. The repo markdown is the only
 * source; titles come from each file's H1. Order here is sidebar order.
 * Plain data, imported by everything else in lib/docs.
 */
export type SectionId = 'start' | 'agents' | 'build' | 'reference' | 'project';

export type PageEntry = {
	slug: string;
	navLabel: string;
	section: SectionId;
	/** repo path, relative to the repo root */
	source: string;
	/** at most 155 characters: what the page answers */
	description: string;
	/** listed under "Optional" in llms.txt and left out of llms-full.txt */
	optional?: boolean;
};

export const SECTIONS: { id: SectionId; title: string }[] = [
	{ id: 'start', title: 'Get started' },
	{ id: 'agents', title: 'Agents' },
	{ id: 'build', title: 'Build on stetdoc' },
	{ id: 'reference', title: 'Reference' },
	{ id: 'project', title: 'Project' },
];

export const PAGES: PageEntry[] = [
	{
		slug: '',
		navLabel: 'Overview',
		section: 'start',
		source: 'docs/README.md',
		description:
			'Where to start with stetdoc, the tool that lets AI agents edit Word files as tracked changes, and what each page of the docs covers.',
	},
	{
		slug: 'quickstart',
		navLabel: 'Quickstart',
		section: 'start',
		source: 'docs/quickstart.md',
		description:
			'Install stetdoc, then find, replace and comment on a sample contract and open the tracked changes in Word. It takes about five minutes.',
	},
	{
		slug: 'install',
		navLabel: 'Install',
		section: 'start',
		source: 'docs/install.md',
		description:
			'Every way to install stetdoc on macOS, Linux and Windows, what each channel costs an agent per call, and how to update, remove or script it.',
	},
	{
		slug: 'concepts',
		navLabel: 'How stetdoc works',
		section: 'start',
		source: 'docs/concepts.md',
		description:
			'The working copy, refs, base tokens and receipts that every stetdoc command relies on, and why stetdoc refuses an edit instead of guessing.',
	},
	{
		slug: 'agents',
		navLabel: 'Use with your agent',
		section: 'agents',
		source: 'docs/agents.md',
		description:
			'Set up stetdoc for Claude Code, Codex, Cursor, OpenCode and other agents with stetdoc init, set the author name, and connect the MCP server.',
	},
	{
		slug: 'skill',
		navLabel: 'The agent skill',
		section: 'agents',
		source: 'crates/stetdoc-cli/skill/core.md',
		description:
			'The skill stetdoc gives an agent: how to read a .docx, edit it with tracked changes, comment, review a returned draft and act on a refusal.',
	},
	{
		slug: 'self-hosting',
		navLabel: 'Self-hosting MCP',
		section: 'agents',
		source: 'docs/self-hosting-mcp.md',
		description:
			'Run stetdoc mcp as a remote MCP server for cloud apps such as Claude Cowork or ChatGPT on the web, with one root per user, auth and TLS.',
	},
	{
		slug: 'build',
		navLabel: 'Build on stetdoc',
		section: 'build',
		source: 'docs/build-on-stetdoc.md',
		description:
			'The four ways to build stetdoc into a product: the Rust crate, the Node and Bun package, the CLI as a subprocess and the MCP server.',
	},
	{
		slug: 'node',
		navLabel: 'Node and Bun',
		section: 'build',
		source: 'crates/stetdoc-napi/npm/README.md',
		description:
			'The npm package @clauseo/stetdoc: open a .docx in Node or Bun, edit it as Word tracked changes and handle its errors, with TypeScript types.',
	},
	{
		slug: 'reference/cli',
		navLabel: 'CLI',
		section: 'reference',
		source: 'docs/reference/cli.md',
		description:
			'Every stetdoc command and flag, the exit codes, the JSON output and the environment variables, with examples that run as tests.',
	},
	{
		slug: 'reference/mcp-tools',
		navLabel: 'MCP tools',
		section: 'reference',
		source: 'docs/reference/mcp-tools.md',
		description:
			'Every tool stetdoc mcp serves, with its arguments and the description that tools/list sends to the agent.',
	},
	{
		slug: 'reference/operations',
		navLabel: 'Operations',
		section: 'reference',
		source: 'docs/reference/operations.md',
		description:
			'Every operation a stetdoc batch can hold, with its fields, whether Word tracks it, and an example in block text.',
	},
	{
		slug: 'reference/errors',
		navLabel: 'Errors',
		section: 'reference',
		source: 'docs/reference/errors.md',
		description: 'Every refusal code stetdoc returns, its exit status, what it means and what to do next.',
	},
	{
		slug: 'reference/findings',
		navLabel: 'Findings',
		section: 'reference',
		source: 'docs/reference/findings.md',
		description:
			'Every problem stetdoc check reports in a document, split into violations and notes, with what each one means.',
	},
	{
		slug: 'fidelity',
		navLabel: 'Fidelity',
		section: 'reference',
		source: 'FIDELITY.md',
		description:
			'What an engineer reads before adopting stetdoc: for each Word feature, what it keeps exactly, normalizes, shows read-only or leaves out.',
	},
	{
		slug: 'glossary',
		navLabel: 'Glossary',
		section: 'reference',
		source: 'GLOSSARY.md',
		description:
			'The exact terms stetdoc uses in its code, docs, errors and agent instructions, grouped by package, document, editing, revisions and formatting.',
	},
	{
		slug: 'troubleshooting',
		navLabel: 'Troubleshooting',
		section: 'reference',
		source: 'docs/troubleshooting.md',
		description:
			"Fixes for the problems people hit most: no author set, ambiguous_match, stale_target, a skill the agent can't see and an MCP server that won't start.",
	},
	{
		slug: 'changelog',
		navLabel: 'Changelog',
		section: 'project',
		source: 'CHANGELOG.md',
		description: 'Every stetdoc release and what changed in it, newest first.',
		optional: true,
	},
	{
		slug: 'security',
		navLabel: 'Security',
		section: 'project',
		source: 'SECURITY.md',
		description:
			'What an engineer reads before adopting stetdoc: how to report a vulnerability, which versions get fixes and how stetdoc tests hostile input.',
	},
	{
		slug: 'evals',
		navLabel: 'Evals',
		section: 'project',
		source: 'docs/evals.md',
		description:
			'The suite that runs Claude Code with the stetdoc plugin on real legal documents and scores its turns, tool calls, tokens, cost and results.',
		optional: true,
	},
];

export const EXTERNAL_ROWS: { label: string; href: string; section: SectionId }[] = [
	{
		label: 'Contributing, on GitHub',
		href: 'https://github.com/clauseo-ai/stetdoc/blob/main/CONTRIBUTING.md',
		section: 'project',
	},
];

