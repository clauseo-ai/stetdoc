/**
 * Reading the repo markdown at build time: the files, their placeholders,
 * their heading ids (github-slugger, so anchors match GitHub's), their git
 * dates, and where each link in them goes on the site. Shared by the HTML
 * renderer (render.ts) and the markdown twins (twin.ts).
 */
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, statSync } from 'node:fs';
import { join, posix } from 'node:path';
import GithubSlugger from 'github-slugger';
import type { Heading, Root } from 'mdast';
import { toString } from 'mdast-util-to-string';
import remarkGfm from 'remark-gfm';
import remarkParse from 'remark-parse';
import { unified } from 'unified';
import { SITE_URL } from '../../app/links';
import { PAGES, type PageEntry } from './table';

export const REPO_ROOT = join(process.cwd(), '..');
export const GITHUB = 'https://github.com/clauseo-ai/stetdoc';
const GITHUB_FILE = /^https:\/\/github\.com\/clauseo-ai\/stetdoc\/(?:blob|tree)\/main\/([^#?]+)(#.*)?$/;

/* ---------- pages and URLs ---------- */

export const pagePath = (slug: string) => (slug ? `/docs/${slug}` : '/docs');
export const twinPath = (slug: string) => (slug ? `/docs/${slug}.md` : '/docs.md');
export const pageBySource = new Map<string, PageEntry>(PAGES.map((p) => [p.source, p]));

/* ---------- the files ---------- */

let versionCache: string | undefined;
/** the workspace version from ../Cargo.toml; the build fails without it */
export function repoVersion(): string {
	if (versionCache) return versionCache;
	const toml = readFileSync(join(REPO_ROOT, 'Cargo.toml'), 'utf8');
	const match = toml.match(/\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/);
	if (!match) throw new Error('docs: no [workspace.package] version in ../Cargo.toml');
	versionCache = match[1];
	return versionCache;
}

/** the table under a heading of a generated file, as the binary writes it */
function tableAfter(source: string, heading: string): string[] {
	const lines = readRaw(source).split('\n');
	const at = lines.indexOf(heading);
	if (at < 0) throw new Error(`docs: ${source} has no "${heading}" for the skill's tables`);
	const rows: string[] = [];
	for (const line of lines.slice(at + 1)) {
		if (line.startsWith('|')) rows.push(line);
		else if (rows.length) break;
	}
	return rows;
}

/**
 * The skill's placeholders, filled the way `stetdoc skills get core` fills
 * them (crates/stetdoc-cli/src/skill.rs), from the generated references the
 * binary wrote: cli.md's command table and operations.md's summary.
 */
const FILLS: Record<string, () => string> = {
	'{{version}}': () => repoVersion(),
	// cli.md links each command to its section; the skill's table doesn't
	'{{commands}}': () =>
		tableAfter('docs/reference/cli.md', '## Commands')
			.map((row) => row.replace(/^\| \[(`[^`]+`)\]\(#[^)]*\) \|/, '| $1 |'))
			.join('\n'),
	'{{ops_summary}}': () => {
		const rows = tableAfter('docs/reference/operations.md', '## Summary').slice(2);
		const body = rows.map((row) => {
			// | [`name`](#name) | does | tracked | reversible |
			const cells = row.split(/(?<!\\)\|/).slice(1, -1).map((c) => c.trim());
			const name = cells[0].replace(/^\[(`[^`]+`)\]\([^)]*\)$/, '$1');
			return `| ${name} | ${cells[2]} | ${cells[1]} |`;
		});
		return ['| Operation | Tracked | Does |', '|---|---|---|', ...body].join('\n');
	},
};

const rawCache = new Map<string, string>();
function readRaw(source: string): string {
	let text = rawCache.get(source);
	if (text === undefined) {
		text = readFileSync(join(REPO_ROOT, source), 'utf8');
		rawCache.set(source, text);
	}
	return text;
}

/** fills placeholders and fails on any that is left */
export function fillPlaceholders(text: string, source = 'markdown'): string {
	let out = text;
	for (const [key, make] of Object.entries(FILLS)) if (out.includes(key)) out = out.split(key).join(make());
	const left = out.match(/\{\{[^{}\n]*\}\}/);
	if (left) throw new Error(`docs: ${source} has an unfilled placeholder ${left[0]}`);
	return out;
}

const textCache = new Map<string, string>();
/** a source file's markdown with its placeholders filled */
export function readSource(source: string): string {
	let text = textCache.get(source);
	if (text === undefined) {
		text = fillPlaceholders(readRaw(source), source);
		textCache.set(source, text);
	}
	return text;
}

/* ---------- parsing and heading ids ---------- */

const parser = unified().use(remarkParse).use(remarkGfm);
export const parseMarkdown = (text: string): Root => parser.runSync(parser.parse(text)) as Root;

export const headingText = (node: Heading) => toString(node);

/** every heading's id in document order, numbered for duplicates as GitHub does */
export function headingIds(tree: Root): { node: Heading; id: string }[] {
	const slugger = new GithubSlugger();
	const out: { node: Heading; id: string }[] = [];
	const walk = (node: { type: string; children?: unknown[] }) => {
		if (node.type === 'heading') out.push({ node: node as Heading, id: slugger.slug(toString(node)) });
		else if (node.type !== 'code') for (const child of (node.children ?? []) as { type: string }[]) walk(child);
	};
	walk(tree);
	return out;
}

const slugCache = new Map<string, Set<string>>();
/** the anchors a repo markdown file has, on GitHub and on the site */
export function anchorsOf(source: string): Set<string> {
	let ids = slugCache.get(source);
	if (!ids) {
		const text = pageBySource.has(source) ? readSource(source) : readRaw(source);
		ids = new Set(headingIds(parseMarkdown(text)).map((h) => h.id));
		slugCache.set(source, ids);
	}
	return ids;
}

const titleCache = new Map<string, string>();
/** a page's H1 text, from its H1 line alone (cheap: the sidebar needs every title) */
export function sourceTitle(source: string): string {
	let title = titleCache.get(source);
	if (title === undefined) {
		const text = readSource(source);
		const line = text.split('\n').find((l) => /^# /.test(l));
		if (!line) throw new Error(`docs: ${source} has no H1`);
		title = titleOf(parseMarkdown(line), source);
		titleCache.set(source, title);
	}
	return title;
}

/** the H1's text; the build fails on a page without one */
export function titleOf(tree: Root, source: string): string {
	const h1 = tree.children.find((n): n is Heading => n.type === 'heading' && n.depth === 1);
	if (!h1) throw new Error(`docs: ${source} has no H1`);
	return toString(h1);
}

/* ---------- git dates ---------- */

const git = (args: string[]) =>
	execFileSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim();

type Dates = { updated?: string; published?: string };
let dateTable: Map<string, Dates> | undefined;

/** every page source's dates, from one git status and one git log */
function loadDates(): Map<string, Dates> {
	const table = new Map<string, Dates>();
	const sources = [...pageBySource.keys()];
	try {
		if (git(['rev-parse', '--is-shallow-repository']) !== 'false') return table;
		const dirty = new Set(
			git(['status', '--porcelain', '--untracked-files=all', '--', ...sources])
				.split('\n')
				.filter(Boolean)
				.map((l) => l.slice(3).replace(/^.* -> /, '')),
		);
		// newest first: "\0<date>" then the files that commit touched
		let date = '';
		for (const line of git(['log', '--format=%x00%cI', '--name-only', '--', ...sources]).split('\n')) {
			if (line.startsWith('\0')) date = line.slice(1);
			else if (line && !dirty.has(line) && pageBySource.has(line)) {
				const d = table.get(line);
				if (d) d.published = date;
				else table.set(line, { updated: date, published: date });
			}
		}
	} catch {
		return new Map();
	}
	return table;
}

/**
 * The source's last and first commit dates, ISO 8601 with timezone. Both are
 * undefined when the clone is shallow, the file is untracked or has
 * uncommitted changes, or git isn't there: no date beats a wrong one.
 */
export function gitDates(source: string): Dates {
	dateTable ??= loadDates();
	return dateTable.get(source) ?? {};
}

/* ---------- links ---------- */

export type ResolvedLink = {
	/** the href on the HTML page */
	html: string;
	/** the href in the markdown twin */
	twin: string;
};

export class LinkError extends Error {}

function exists(repoPath: string): 'file' | 'dir' | null {
	const full = join(REPO_ROOT, repoPath);
	if (!existsSync(full)) return null;
	return statSync(full).isDirectory() ? 'dir' : 'file';
}

function checkAnchor(target: string, anchor: string, from: string, href: string) {
	if (!anchor || !target.endsWith('.md')) return;
	if (!anchorsOf(target).has(anchor))
		throw new LinkError(`docs: ${from} links to ${href}, but ${target} has no heading with the id "${anchor}"`);
}

function toPage(page: PageEntry, anchor: string): ResolvedLink {
	const hash = anchor ? `#${anchor}` : '';
	return { html: `${pagePath(page.slug)}${hash}`, twin: `${SITE_URL}${twinPath(page.slug)}${hash}` };
}

/**
 * Where a link in a source file goes. Relative links resolve against the
 * file's folder: a page in the table goes to its /docs URL, any other repo
 * file or folder to GitHub. An absolute GitHub link to a file in the table
 * goes to the site. The build fails on a relative link to a missing file
 * or to a heading that isn't there. Same-page anchors are checked against
 * ownAnchors when given (the text being rendered), else the file on disk.
 */
export function resolveLink(href: string, from: string, ownAnchors?: Set<string>): ResolvedLink {
	if (href.startsWith('#')) {
		const anchor = decodeURIComponent(href.slice(1));
		if (ownAnchors) {
			if (anchor && !ownAnchors.has(anchor)) throw new LinkError(`docs: ${from} links to ${href}, but has no heading with the id "${anchor}"`);
		} else checkAnchor(from, anchor, from, href);
		return { html: href, twin: href };
	}
	const github = href.match(GITHUB_FILE);
	if (github) {
		const page = pageBySource.get(decodeURIComponent(github[1]));
		if (page) {
			const anchor = github[2] ? decodeURIComponent(github[2].slice(1)) : '';
			checkAnchor(page.source, anchor, from, href);
			return toPage(page, anchor);
		}
		return { html: href, twin: href };
	}
	if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//')) return { html: href, twin: href };

	const [pathPart, anchorPart = ''] = href.split('#', 2);
	const clean = decodeURIComponent(pathPart.split('?')[0]);
	const target = posix.normalize(posix.join(posix.dirname(from), clean)).replace(/\/$/, '');
	if (target.startsWith('..')) throw new LinkError(`docs: ${from} links to ${href}, outside the repo`);
	const kind = exists(target);
	if (!kind) throw new LinkError(`docs: ${from} links to ${href}, but ${target} doesn't exist`);
	const anchor = decodeURIComponent(anchorPart);
	const page = pageBySource.get(target);
	if (page) {
		checkAnchor(target, anchor, from, href);
		return toPage(page, anchor);
	}
	if (kind === 'dir') {
		const url = `${GITHUB}/tree/main/${target}`;
		return { html: url, twin: url };
	}
	checkAnchor(target, anchor, from, href);
	const url = `${GITHUB}/blob/main/${target}${anchorPart ? `#${anchorPart}` : ''}`;
	return { html: url, twin: url };
}

