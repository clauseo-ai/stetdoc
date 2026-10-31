/**
 * The markdown twin of a page, served at /docs/<slug>.md and gathered into
 * llms-full.txt: the source markdown with its links made absolute (site
 * twins, or GitHub for files not on the site), placeholders filled, HTML
 * comments removed, and a short header under the H1: the description and
 * a Source line with the page URL, the last update and the version.
 */
import type { Definition, Html, Link, Root } from 'mdast';
import { SITE_URL } from '../../app/links';
import {
	gitDates,
	headingIds,
	pagePath,
	parseMarkdown,
	readSource,
	repoVersion,
	resolveLink,
	titleOf,
} from './source';
import type { PageEntry } from './table';

type Edit = { start: number; end: number; text: string };

const offsets = (node: { position?: { start: { offset?: number }; end: { offset?: number } } }) => {
	const start = node.position?.start.offset;
	const end = node.position?.end.offset;
	if (start === undefined || end === undefined) throw new Error('docs: a markdown node without a position');
	return { start, end };
};

/** the edit that swaps a link's destination in the source text */
function destinationEdit(text: string, node: Link | Definition, to: string): Edit | null {
	const { start, end } = offsets(node);
	const slice = text.slice(start, end);
	let at: number;
	if (node.type === 'definition') {
		at = slice.indexOf(']:') + 2;
		while (/\s/.test(slice[at])) at++;
	} else {
		const open = slice.lastIndexOf('](');
		if (open < 0) {
			// an autolink: the whole node is the URL, maybe in <>
			const inner = slice.startsWith('<') ? 1 : 0;
			return { start: start + inner, end: end - inner, text: to };
		}
		at = open + 2;
		while (/\s/.test(slice[at])) at++;
	}
	if (slice[at] === '<') {
		const close = slice.indexOf('>', at);
		return { start: start + at + 1, end: start + close, text: to };
	}
	let stop = at;
	let depth = 0;
	while (stop < slice.length) {
		const c = slice[stop];
		if (/\s/.test(c)) break;
		if (c === '(') depth++;
		if (c === ')') {
			if (depth === 0) break;
			depth--;
		}
		stop++;
	}
	return { start: start + at, end: start + stop, text: to };
}

function apply(text: string, edits: Edit[]): string {
	let out = text;
	for (const e of [...edits].sort((a, b) => b.start - a.start)) out = out.slice(0, e.start) + e.text + out.slice(e.end);
	return out;
}

/**
 * The twin's text from a source's markdown. Exported for the tests; pages
 * go through pageTwin.
 */
export function twinMarkdown(
	text: string,
	source: string,
	header: { description: string; url: string; updated?: string; version: string },
): string {
	const tree: Root = parseMarkdown(text);
	const own = new Set(headingIds(tree).map((h) => h.id));
	const edits: Edit[] = [];
	const walk = (node: { type: string; children?: unknown[] }) => {
		if (node.type === 'link' || node.type === 'definition') {
			const link = node as Link | Definition;
			const to = resolveLink(link.url, source, own).twin;
			if (to !== link.url) {
				const edit = destinationEdit(text, link, to);
				if (edit) edits.push(edit);
			}
		}
		if (node.type === 'html' && /^<!--[\s\S]*-->$/.test((node as Html).value.trim())) {
			const { start } = offsets(node as Html);
			let { end } = offsets(node as Html);
			// a comment on lines of its own takes its blank lines with it
			if (start === 0 || text[start - 1] === '\n') while (text[end] === '\n') end++;
			edits.push({ start, end, text: '' });
		}
		for (const child of (node.children ?? []) as { type: string }[]) walk(child);
	};
	walk(tree);

	const h1 = tree.children.find((n) => n.type === 'heading' && n.depth === 1);
	if (!h1) throw new Error(`docs: ${source} has no H1`);
	const date = header.updated ? ` · Updated ${header.updated.slice(0, 10)}` : '';
	const lines = ['', '', `> ${header.description}`, '', `Source: ${header.url}${date} · stetdoc ${header.version}`];
	const after = offsets(h1).end;
	edits.push({ start: after, end: after, text: lines.join('\n') });
	return `${apply(text, edits).trimEnd()}\n`;
}

const twinCache = new Map<string, { title: string; markdown: string; updated?: string; published?: string }>();

export function pageTwin(page: PageEntry) {
	const hit = twinCache.get(page.slug);
	if (hit) return hit;
	const text = readSource(page.source);
	const title = titleOf(parseMarkdown(text), page.source);
	const { updated, published } = gitDates(page.source);
	const markdown = twinMarkdown(text, page.source, {
		description: page.description,
		url: `${SITE_URL}${pagePath(page.slug)}`,
		updated,
		version: repoVersion(),
	});
	const twin = { title, markdown, updated, published };
	twinCache.set(page.slug, twin);
	return twin;
}
