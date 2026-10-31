/**
 * A page's markdown to the article's hast, in the contract's markup
 * (section 2): links resolved, GitHub alerts as callouts, headings with ids
 * and self-links, a lead paragraph, code figures, labelled tables, key-value
 * lists and, on the overview, the doors. Raw HTML in the markdown is shown
 * as text, never rendered.
 */
import type { Element, ElementContent, Root as HastRoot, RootContent } from 'hast';
import type { Blockquote, Paragraph, Root, RootContent as MdContent, Text } from 'mdast';
import { toString as mdToString } from 'mdast-util-to-string';
import remarkRehype from 'remark-rehype';
import { unified } from 'unified';
import { codeFigure } from './code';
import { headingIds, parseMarkdown, resolveLink } from './source';
import { CALLOUT, EXTERNAL } from './strings';

export type TocEntry = { id: string; text: string; depth: 2 | 3 };

export type RenderOptions = {
	/** the overview: wrap the "Start here" paragraphs as doors */
	doors?: boolean;
	/** the changelog: the TOC lists h2 only */
	tocH2Only?: boolean;
	/** the glossary: a paragraph that opens with a bold term is an entry */
	glossary?: boolean;
};

/* ---------- mdast ---------- */

type MdParent = { type: string; children?: MdContent[] };

function mdWalk(node: MdParent, visit: (node: MdContent, parent: MdParent, index: number) => void) {
	const kids = node.children ?? [];
	for (let i = 0; i < kids.length; i++) {
		visit(kids[i], node, i);
		mdWalk(kids[i] as MdParent, visit);
	}
}

/** `> [!NOTE]` and its kin → an aside with a visible label */
function alerts(tree: Root) {
	mdWalk(tree, (node) => {
		if (node.type !== 'blockquote') return;
		const quote = node as Blockquote;
		const first = quote.children[0];
		if (first?.type !== 'paragraph') return;
		const lead = first.children[0];
		if (lead?.type !== 'text') return;
		const m = lead.value.match(/^\[!(NOTE|TIP|IMPORTANT|WARNING|CAUTION)\][ \t]*(?:\n|$)/);
		if (!m) return;
		const callout = CALLOUT[m[1] as keyof typeof CALLOUT];
		lead.value = lead.value.slice(m[0].length);
		if (!lead.value) first.children.shift();
		if (first.children[0]?.type === 'break') first.children.shift();
		if (!first.children.length) quote.children.shift();
		const label: Paragraph = {
			type: 'paragraph',
			children: [{ type: 'text', value: callout.label }],
			data: { hProperties: { className: ['callout-label'] } },
		};
		quote.children.unshift(label);
		quote.data = { hName: 'aside', hProperties: { className: ['callout'], dataKind: callout.kind, role: 'note' } };
	});
}

/** relative and in-table links → site URLs; the build fails on a broken one */
function links(tree: Root, source: string, own: Set<string>) {
	mdWalk(tree, (node) => {
		if (node.type === 'link' || node.type === 'definition') node.url = resolveLink(node.url, source, own).html;
	});
}

/** raw HTML shows as the text it is; comments go */
function escapeHtml(tree: Root) {
	mdWalk(tree, (node, parent, index) => {
		if (node.type !== 'html') return;
		const kids = parent.children!;
		if (/^<!--[\s\S]*-->$/.test(node.value.trim())) {
			kids.splice(index, 1, { type: 'text', value: '' } as Text);
			return;
		}
		const asText: Text = { type: 'text', value: node.value };
		const inline = parent.type === 'paragraph' || parent.type === 'heading' || parent.type === 'tableCell' || parent.type === 'emphasis' || parent.type === 'strong' || parent.type === 'link';
		kids[index] = (inline ? asText : ({ type: 'paragraph', children: [asText] } as Paragraph)) as MdContent;
	});
}

/* ---------- hast ---------- */

const text = (value: string): ElementContent => ({ type: 'text', value });
const el = (tagName: string, properties: Element['properties'], children: ElementContent[]): Element => ({
	type: 'element',
	tagName,
	properties,
	children,
});
const isEl = (node: unknown, tag?: string): node is Element =>
	(node as Element)?.type === 'element' && (!tag || (node as Element).tagName === tag);
const hastText = (node: RootContent | ElementContent): string =>
	node.type === 'text' ? node.value : 'children' in node ? node.children.map((c) => hastText(c as ElementContent)).join('') : '';
const elementKids = (node: Element) => node.children.filter((c): c is Element => isEl(c));

/** a heading's text with inner links flattened, so the self-link doesn't nest */
function unlink(children: ElementContent[]): ElementContent[] {
	return children.flatMap((c): ElementContent[] => {
		if (!isEl(c)) return [c];
		if (c.tagName === 'a') return unlink(c.children);
		return [{ ...c, children: unlink(c.children) }];
	});
}

function table(node: Element, label: string): Element {
	const head = node.children.find((c): c is Element => isEl(c, 'thead'));
	const headers = head ? elementKids(elementKids(head)[0] ?? el('tr', {}, [])).map((th) => hastText(th).trim()) : [];
	const bodyRows = node.children
		.filter((c): c is Element => isEl(c, 'tbody'))
		.flatMap((b) => elementKids(b).filter((r) => isEl(r, 'tr')));
	const align = (cell: Element) => {
		const a = cell.properties.align;
		delete cell.properties.align;
		if (a === 'right' || a === 'center') cell.properties.dataAlign = a;
	};
	if (head) for (const th of elementKids(elementKids(head)[0] ?? el('tr', {}, []))) align(th);
	let keyed = bodyRows.length > 0;
	for (const row of bodyRows) {
		elementKids(row).forEach((td, i) => {
			align(td);
			if (headers[i] !== undefined) td.properties.dataLabel = headers[i];
			if (i === 0) {
				const meaningful = td.children.filter((c) => !(c.type === 'text' && !c.value.trim()));
				if (!(meaningful.length === 1 && isEl(meaningful[0], 'code'))) keyed = false;
			}
		});
	}
	if (keyed) node.properties.dataKeyed = '';
	// phone cards make table parts display:block, which drops their implicit
	// roles in WebKit; explicit roles keep the table a table to VoiceOver
	node.properties.role = 'table';
	for (const group of elementKids(node)) {
		if (group.tagName !== 'thead' && group.tagName !== 'tbody' && group.tagName !== 'tfoot') continue;
		group.properties.role = 'rowgroup';
		for (const row of elementKids(group)) {
			row.properties.role = 'row';
			elementKids(row).forEach((cell, i) => {
				if (cell.tagName === 'th') cell.properties.role = group.tagName === 'thead' ? 'columnheader' : keyed && i === 0 ? 'rowheader' : 'cell';
				else cell.properties.role = 'cell';
			});
		}
	}
	return el('div', { className: ['table'], role: 'region', tabIndex: 0, ariaLabel: label }, [node]);
}

/** a list where every item opens with <strong>Key:</strong> */
function kvList(node: Element): boolean {
	const items = elementKids(node).filter((c) => isEl(c, 'li'));
	if (!items.length) return false;
	const leads: Element[] = [];
	for (const li of items) {
		let host: Element = li;
		const first = li.children.find((c) => !(c.type === 'text' && !c.value.trim()));
		if (isEl(first, 'p')) host = first;
		const lead = host.children.find((c) => !(c.type === 'text' && !c.value.trim()));
		if (!isEl(lead, 'strong') || !/:\s*$/.test(hastText(lead))) return false;
		leads.push(lead);
	}
	node.properties.className = ['kv'];
	for (const strong of leads) {
		strong.properties.className = ['kv-key'];
		const last = strong.children[strong.children.length - 1];
		if (last?.type === 'text') last.value = last.value.replace(/:\s*$/, '');
		strong.children.push(el('span', { className: ['sr-only'] }, [text(':')]));
	}
	return true;
}

function external(node: Element) {
	const href = String(node.properties.href ?? '');
	if (!/^https?:\/\//.test(href)) return;
	const github = href.startsWith('https://github.com/');
	node.properties.dataExternal = github ? 'github' : 'site';
	node.children.push(el('span', { className: ['sr-only'] }, [text(github ? EXTERNAL.github : EXTERNAL.site)]));
}

/**
 * The article after the H1, as hast, with its TOC. Throws on a broken link
 * or anchor, a page without an H1, or any leftover problem the build should
 * catch.
 */
export function renderMarkdown(markdown: string, source: string, options: RenderOptions = {}): { tree: HastRoot; toc: TocEntry[]; title: string } {
	const mdast = parseMarkdown(markdown);
	const ids = new Map(headingIds(mdast).map((h) => [h.node, h.id]));
	const h1Index = mdast.children.findIndex((n) => n.type === 'heading' && n.depth === 1);
	if (h1Index < 0) throw new Error(`docs: ${source} has no H1`);
	const title = mdToString(mdast.children[h1Index]);
	// heading ids from the full document, so numbering matches GitHub's
	mdWalk(mdast, (node) => {
		if (node.type === 'heading') node.data = { ...node.data, hProperties: { id: ids.get(node)! } };
	});
	alerts(mdast);
	links(mdast, source, new Set(ids.values()));
	escapeHtml(mdast);
	mdast.children = mdast.children.slice(h1Index + 1);

	const hast = unified().use(remarkRehype, { allowDangerousHtml: false }).runSync(mdast) as HastRoot;

	const toc: TocEntry[] = [];
	let nearest = title;
	// every region (scrolling code, tables) needs its own accessible name
	const names = new Map<string, number>();
	const unique = (name: string) => {
		const n = (names.get(name) ?? 0) + 1;
		names.set(name, n);
		return n === 1 ? name : `${name} (${n})`;
	};
	const visit = (node: Element | HastRoot) => {
		node.children = (node.children as ElementContent[]).map((child) => {
			if (!isEl(child)) return child;
			const tag = child.tagName;
			if (/^h[2-6]$/.test(tag)) {
				const id = String(child.properties.id);
				const words = hastText(child).trim();
				nearest = words;
				const depth = Number(tag[1]);
				if ((depth === 2 || (depth === 3 && !options.tocH2Only)) && !(depth === 3 && words === 'Examples'))
					toc.push({ id, text: words, depth: depth as 2 | 3 });
				if (depth <= 4) child.children = [el('a', { className: ['anchor'], href: `#${id}` }, unlink(child.children))];
				return child;
			}
			if (tag === 'pre') {
				const code = elementKids(child)[0];
				const cls = (code?.properties.className as string[] | undefined)?.find((c) => c.startsWith('language-'));
				const fig = codeFigure(cls?.slice('language-'.length), code ? hastText(code) : hastText(child));
				const pre = elementKids(fig).find((e) => e.tagName === 'pre');
				if (pre?.properties.role === 'region') pre.properties.ariaLabel = unique(`${pre.properties.ariaLabel}, ${nearest}`);
				return fig;
			}
			if (tag === 'table') {
				visit(child);
				return table(child, unique(nearest));
			}
			if (tag === 'code' && hastText(child).length > LONG_CODE) {
				child.properties.className = ['long'];
				child.children = wordSpans(hastText(child));
				return child;
			}
			if (tag === 'code') return child;
			if (tag === 'a') external(child);
			if (tag === 'ul') kvList(child);
			visit(child);
			return child;
		});
	};
	visit(hast);

	// strip the whitespace text nodes between top-level blocks
	hast.children = hast.children.filter((c) => !(c.type === 'text' && !c.value.trim()));
	const first = hast.children[0];
	if (isEl(first, 'p') && hastText(first).trim().length <= LEAD_MAX) first.properties.className = ['lead'];

	if (options.doors) doors(hast);
	if (options.glossary) glossaryEntries(hast);
	mergeText(hast);
	keepUnits(hast);
	keepFlags(hast);
	return { tree: hast, toc, title };
}

/** inline code longer than this may wrap (class "long"); shorter code never breaks */
export const LONG_CODE = 28;
/** a first paragraph longer than this is body text, not a lead */
export const LEAD_MAX = 240;

/**
 * Long inline code's words: each space-separated token of up to LONG_CODE
 * characters in <span class="w"> (kept whole), the spaces between as plain
 * text where lines can break. A longer token, a path or a URL, is cut after
 * each slash (never inside "//") into pieces kept whole the same way, with
 * a <wbr> between them, so it breaks only at a slash; a piece still longer
 * than LONG_CODE stays bare and may break anywhere rather than overflow.
 * The text is unchanged.
 */
export function wordSpans(value: string): ElementContent[] {
	const whole = (part: string): ElementContent =>
		part.length > LONG_CODE ? text(part) : el('span', { className: ['w'] }, [text(part)]);
	return value
		.split(/( +)/)
		.filter((part) => part !== '')
		.flatMap((part): ElementContent[] => {
			if (part.startsWith(' ')) return [text(part)];
			if (part.length <= LONG_CODE) return [whole(part)];
			return part
				.split(/(?<=\/)(?!\/)/)
				.flatMap((piece, i) => (i ? [el('wbr', {}, []), whole(piece)] : [whole(piece)]));
		});
}

/** a number and its unit: the space between them becomes U+00A0 */
const UNIT = /(\d) (ms|s|min|KB|MB|GB|kB|px|%)(?![\p{L}\p{N}])/gu;
export const bindUnits = (value: string) => value.replace(UNIT, '$1\u00a0$2');

/** binds numbers to their units in prose text, never inside code */
function keepUnits(node: Element | HastRoot) {
	for (const child of node.children) {
		if (child.type === 'text') child.value = bindUnits(child.value);
		else if (child.type === 'element' && child.tagName !== 'code' && child.tagName !== 'pre' && child.tagName !== 'figure') keepUnits(child);
	}
}

/** a --flag in running text, outside code: WebKit breaks a line after a
 * hyphen, which splits --ops into "--" and "ops" */
const FLAG = /(?<![\p{L}\p{N}_-])--[\p{L}\p{N}][\p{L}\p{N}_-]*/gu;
const HAS_FLAG = new RegExp(FLAG.source, 'u');

/** splits a text into plain runs and flags, each flag in <span class="nobr"> */
export function flagSpans(value: string): ElementContent[] {
	const out: ElementContent[] = [];
	let at = 0;
	for (const m of value.matchAll(FLAG)) {
		if (m.index > at) out.push(text(value.slice(at, m.index)));
		out.push(el('span', { className: ['nobr'] }, [text(m[0])]));
		at = m.index + m[0].length;
	}
	if (at < value.length) out.push(text(value.slice(at)));
	return out;
}

/** keeps each --flag in prose text on one line, never inside code */
function keepFlags(node: Element | HastRoot) {
	const kids = node.children as ElementContent[];
	node.children = kids.flatMap((child): ElementContent[] => {
		if (child.type === 'text') return HAS_FLAG.test(child.value) ? flagSpans(child.value) : [child];
		if (child.type === 'element' && child.tagName !== 'code' && child.tagName !== 'pre' && child.tagName !== 'figure') keepFlags(child);
		return [child];
	});
}

/** joins neighbouring text nodes, which React would otherwise split with <!-- --> */
function mergeText(node: Element | HastRoot) {
	const out: (typeof node.children)[number][] = [];
	for (const child of node.children) {
		const prev = out[out.length - 1];
		if (child.type === 'text' && prev?.type === 'text') prev.value += child.value;
		else {
			if (child.type === 'element') mergeText(child);
			out.push(child);
		}
	}
	node.children = out as typeof node.children;
}

/**
 * The glossary's entries: a paragraph that opens with a bold term ending in
 * a full stop ("**Package.** The .docx file…") is class "gl-entry", its
 * term class "gl-term", so the term can hang in its own column beside the
 * definition. Its "*Avoid:*" note, when it has one, and everything after
 * it go in <span class="gl-avoid">, to start a line of its own. Text
 * unchanged. (globals.css owns .term, the launch page's terminal.)
 */
function glossaryEntries(hast: HastRoot) {
	for (const p of hast.children) {
		if (!isEl(p, 'p')) continue;
		const key = p.children[0];
		if (!isEl(key, 'strong') || !/\.\s*$/.test(hastText(key))) continue;
		p.properties.className = ['gl-entry'];
		key.properties.className = ['gl-term'];
		const at = p.children.findIndex((c) => isEl(c, 'em') && hastText(c).trim() === 'Avoid:');
		if (at > 0) p.children.splice(at, p.children.length - at, el('span', { className: ['gl-avoid'] }, p.children.slice(at)));
	}
}

/** the overview's "Start here" paragraphs that open with <strong>, as doors */
function doors(hast: HastRoot) {
	const kids = hast.children;
	const at = kids.findIndex((c) => isEl(c, 'h2') && hastText(c).trim() === 'Start here');
	if (at < 0) throw new Error('docs: the overview has no "Start here" heading for its doors');
	let end = at + 1;
	while (end < kids.length && !(isEl(kids[end]) && /^h[1-6]$/.test((kids[end] as Element).tagName))) end++;
	const out: RootContent[] = [];
	let group: Element | null = null;
	for (const child of kids.slice(at + 1, end)) {
		const isDoor = isEl(child, 'p') && isEl(child.children[0], 'strong');
		if (isDoor) {
			if (!group) {
				group = el('div', { className: ['doors'] }, []);
				out.push(group);
			}
			group.children.push(el('div', { className: ['door'] }, [child as Element]));
		} else {
			group = null;
			out.push(child);
		}
	}
	kids.splice(at + 1, end - at - 1, ...out);
}
