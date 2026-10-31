/*
 * Checks the built docs (out/) after strip-runtime and pagefind, and fails
 * the build on any miss:
 *
 * - every page has its HTML, its markdown twin and its OG card;
 * - internal links and anchors resolve, in the HTML and in the twins;
 * - html-validate passes (Next's inline <style> in <head> allowed);
 * - one h1, a description of at most 155 characters, the right canonical;
 * - no framework script or RSC payload on a docs page, no shiki in out/;
 * - the budgets: HTML ≤ 60 KB gzipped, own inline JS ≤ 4 KB gzipped per
 *   page, llms.txt < 10 KB, llms-full.txt ≤ 400 KB, twins ≤ 120 KB.
 *
 * Prints a size table. Usage: bun scripts/check-docs.ts.
 */
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { gzipSync } from 'node:zlib';
import { HtmlValidate } from 'html-validate';
import { SITE_URL } from '../app/links';
import { DOCS } from '../lib/docs/manifest';

const out = join(import.meta.dir, '..', 'out');
const failures: string[] = [];
const fail = (why: string) => failures.push(why);
const KB = 1024;
const BUDGET = { html: 60 * KB, js: 4 * KB, llms: 10_000, full: 400_000, twin: 120 * KB };

const htmlFile = (slug: string) => (slug ? `docs/${slug}.html` : 'docs.html');
const twinFile = (slug: string) => (slug ? `docs/${slug}.md` : 'docs.md');
const ogFile = (slug: string) => `docs/og/${slug || 'index'}.png`;
const pagePath = (slug: string) => (slug ? `/docs/${slug}` : '/docs');
const gz = (text: string) => gzipSync(text, { level: 9 }).length;
const read = (rel: string) => (existsSync(join(out, rel)) ? readFileSync(join(out, rel), 'utf8') : null);

function walk(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});
}

const decode = (s: string) =>
	s.replace(/&quot;/g, '"').replace(/&#x27;|&#39;/g, "'").replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');

/* ---------- what each page has: ids, for the anchor checks ---------- */

const pages = new Map<string, { html: string; ids: Set<string> }>();
for (const doc of DOCS) {
	const html = read(htmlFile(doc.slug));
	if (html === null) {
		fail(`missing out/${htmlFile(doc.slug)}`);
		continue;
	}
	const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => decode(m[1])));
	pages.set(pagePath(doc.slug), { html, ids });
}

/** checks an internal link: the page exists, and so does its anchor */
function checkLink(href: string, where: string, selfPath: string) {
	const [rawPath, rawHash] = href.split('#', 2);
	const path = rawPath === '' ? selfPath : rawPath === '/' ? '/' : rawPath.replace(/\/$/, '');
	const anchor = rawHash ? decodeURIComponent(rawHash) : '';
	if (path.endsWith('.md')) {
		if (read(path.slice(1)) === null) fail(`${where}: broken link ${href}`);
		const htmlPath = path === '/docs.md' ? '/docs' : path.slice(0, -3);
		if (anchor && pages.has(htmlPath) && !pages.get(htmlPath)!.ids.has(anchor)) fail(`${where}: no #${anchor} on ${htmlPath}`);
		return;
	}
	if (path.startsWith('/docs/og/') || path.startsWith('/pagefind/')) return;
	const page = pages.get(path);
	if (!page) {
		if (!path.startsWith('/docs') && read(path === '/' ? 'index.html' : `${path.slice(1)}.html`) !== null) return;
		if (read(path.slice(1)) !== null) return;
		fail(`${where}: broken link ${href}`);
		return;
	}
	if (anchor && !page.ids.has(anchor)) fail(`${where}: no #${anchor} on ${path}`);
}

/* ---------- per page ---------- */

const validator = new HtmlValidate({
	extends: ['html-validate:recommended'],
	rules: {
		// Next writes these into <head> and the inlined CSS; nothing in the article does
		'no-inline-style': 'off',
		'require-sri': 'off',
		'void-style': 'off',
		'no-trailing-whitespace': 'off',
		'attribute-boolean-style': 'off',
		'attribute-empty-style': 'off',
		'no-redundant-role': 'off',
		'long-title': 'off',
		// Next writes <meta charSet>
		'attr-case': 'off',
		// the contract's tables are <div role="region"> wrappers, as GitHub's are
		'prefer-native-element': ['error', { exclude: ['region'] }],
		// GitHub's ids, such as the changelog's 090---2026-10-30, start with a digit; HTML allows it
		'valid-id': ['error', { relaxed: true }],
	},
});

type Row = { page: string; html: number; js: number; twin: number };
const rows: Row[] = [];

for (const doc of DOCS) {
	const where = htmlFile(doc.slug);
	const page = pages.get(pagePath(doc.slug));
	const twin = read(twinFile(doc.slug));
	if (twin === null) fail(`${where}: no twin out/${twinFile(doc.slug)}`);
	else if (twin !== doc.markdown) fail(`${twinFile(doc.slug)} differs from Doc.markdown`);
	if (!existsSync(join(out, ogFile(doc.slug)))) fail(`${where}: no OG card out/${ogFile(doc.slug)}`);
	if (doc.summary.length > 155) fail(`${where}: description is ${doc.summary.length} characters`);
	if (!page) continue;
	const { html } = page;

	const h1s = html.match(/<h1[\s>]/g)?.length ?? 0;
	if (h1s !== 1) fail(`${where}: ${h1s} h1 elements`);
	const canonical = html.match(/<link rel="canonical" href="([^"]+)"/)?.[1];
	if (canonical !== `${SITE_URL}${pagePath(doc.slug)}`) fail(`${where}: canonical is ${canonical}`);
	const description = html.match(/<meta name="description" content="([^"]*)"/)?.[1];
	if (!description || decode(description) !== doc.summary) fail(`${where}: meta description missing or not the manifest's`);
	if (/<script\b[^>]*\/_next\/static\/chunks\//.test(html)) fail(`${where}: a framework script`);
	if (html.includes('self.__next_f')) fail(`${where}: the RSC payload`);
	const article = html.match(/<article\b[\s\S]*<\/article>/)?.[0] ?? '';
	if (/\sstyle="/.test(article)) fail(`${where}: an inline style in the article`);
	if (!html.includes(`og/${doc.slug || 'index'}.png`)) fail(`${where}: no og:image of its own`);

	for (const m of html.matchAll(/<a\b[^>]*\bhref="([^"]+)"/g)) {
		const href = decode(m[1]);
		if (href.startsWith('/') || href.startsWith('#')) checkLink(href, where, pagePath(doc.slug));
	}

	const report = await validator.validateString(html, where);
	for (const r of report.results)
		for (const msg of r.messages) fail(`${where}:${msg.line}:${msg.column} html-validate ${msg.ruleId}: ${msg.message}`);

	const inlineJs = [...html.matchAll(/<script\b(?![^>]*application\/ld\+json)[^>]*>([\s\S]*?)<\/script>/g)].map((m) => m[1]).join('\n');
	const row = { page: pagePath(doc.slug), html: gz(html), js: inlineJs ? gz(inlineJs) : 0, twin: twin ? Buffer.byteLength(twin) : 0 };
	rows.push(row);
	if (row.html > BUDGET.html) fail(`${where}: ${row.html} bytes gzipped, over ${BUDGET.html}`);
	if (row.js > BUDGET.js) fail(`${where}: inline JS ${row.js} bytes gzipped, over ${BUDGET.js}`);
	if (row.twin > BUDGET.twin) fail(`${twinFile(doc.slug)}: ${row.twin} bytes, over ${BUDGET.twin}`);
}

/* ---------- twins: their links point at real pages and anchors ---------- */

for (const doc of DOCS) {
	const twin = read(twinFile(doc.slug));
	if (!twin) continue;
	for (const m of twin.matchAll(/\]\((https:\/\/stetdoc\.clauseo\.chat[^)\s]*)\)/g)) {
		checkLink(m[1].slice(SITE_URL.length), twinFile(doc.slug), pagePath(doc.slug));
	}
	for (const m of twin.matchAll(/\]\((?!https?:|#|mailto:)([^)\s]+)\)/g)) fail(`${twinFile(doc.slug)}: relative link ${m[1]}`);
}

/* ---------- site files and shiki ---------- */

const llms = read('llms.txt') ?? '';
const full = read('llms-full.txt') ?? '';
if (!llms) fail('missing out/llms.txt');
if (Buffer.byteLength(llms) >= BUDGET.llms) fail(`llms.txt is ${Buffer.byteLength(llms)} bytes`);
if (Buffer.byteLength(full) > BUDGET.full) fail(`llms-full.txt is ${Buffer.byteLength(full)} bytes`);
for (const doc of DOCS) {
	const url = `${SITE_URL}${doc.slug ? `/docs/${doc.slug}.md` : '/docs.md'}`;
	if (!llms.includes(`(${url})`)) fail(`llms.txt doesn't list ${url}`);
	if (!doc.optional && !full.includes(doc.markdown.trim())) fail(`llms-full.txt lacks the twin of ${pagePath(doc.slug)}`);
}

for (const file of walk(out)) {
	if (!/\.(html|js|css|txt|md|json)$/.test(file)) continue;
	if (/shiki/i.test(readFileSync(file, 'utf8'))) fail(`shiki in out/${relative(out, file)}`);
}

/* ---------- report ---------- */

const pad = (s: string | number, n: number) => String(s).padStart(n);
console.log(`check-docs: ${'page'.padEnd(28)} ${pad('html gz', 8)} ${pad('js gz', 6)} ${pad('twin', 7)}`);
for (const r of rows) console.log(`            ${r.page.padEnd(28)} ${pad(r.html, 8)} ${pad(r.js, 6)} ${pad(r.twin, 7)}`);
console.log(`            llms.txt ${Buffer.byteLength(llms)} bytes, llms-full.txt ${Buffer.byteLength(full)} bytes`);

if (failures.length) {
	console.error(`check-docs: failed\n  ${failures.join('\n  ')}`);
	process.exit(1);
}
console.log(`check-docs: ${DOCS.length} pages, twins, cards, links, HTML and budgets pass`);
