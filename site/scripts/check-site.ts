/*
 * Checks the built site (out/) for what agents and search engines rely on,
 * after strip-runtime. Fails the build on any miss.
 *
 * - The launch page and its markdown twin carry the same install command,
 *   version and agent install block, and llms.txt carries the block too.
 * - Every page has its own canonical and a markdown alternate.
 * - robots.txt, sitemap.xml, llms.txt, llms-full.txt and security.txt exist,
 *   with absolute URLs and within size.
 */
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { SITE_URL } from '../app/links';
import { AGENT_BLOCK, SITE } from '../content/copy';
import { sitePages } from '../lib/pages';
import { stetdocVersion } from '../lib/version';

const out = join(import.meta.dir, '..', 'out');
const failures: string[] = [];
const read = (path: string) => {
	const file = join(out, path);
	if (!existsSync(file)) {
		failures.push(`missing out/${path}`);
		return '';
	}
	return readFileSync(file, 'utf8');
};
const decode = (html: string) =>
	html
		.replace(/<[^>]+>/g, '')
		.replace(/&quot;/g, '"')
		.replace(/&#x27;|&#39;/g, "'")
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.replace(/&amp;/g, '&');
const need = (where: string, text: string, what: string) => {
	if (!text.includes(what)) failures.push(`${where} lacks: ${what.slice(0, 80)}`);
};

const html = read('index.html');
const htmlText = decode(html);
const md = read('index.md');
const llms = read('llms.txt');
const version = stetdocVersion();

for (const [where, text] of [
	['index.html', htmlText],
	['index.md', md],
	['llms.txt', llms],
] as const) {
	need(where, text, SITE.install);
	for (const line of AGENT_BLOCK.text.split('\n')) need(where, text, line.trim());
}
need('index.html', html, `"softwareVersion":"${version}"`);
need('index.md', md, `Version ${version}`);
if (llms.length > 10_000) failures.push(`llms.txt is ${llms.length} bytes, over 10 KB`);
const full = read('llms-full.txt');
if (full.length > 400_000) failures.push(`llms-full.txt is ${full.length} bytes`);

// per page: its own canonical and its markdown alternate
for (const page of sitePages()) {
	const file = page.path === '/' ? 'index.html' : `${page.path.slice(1)}.html`;
	const text = read(file);
	if (!text) continue;
	const canonical = text.match(/<link rel="canonical" href="([^"]+)"/)?.[1] ?? '';
	const want = page.path === '/' ? SITE_URL : `${SITE_URL}${page.path}`;
	if (canonical.replace(/\/$/, '') !== want) failures.push(`${file} canonical is "${canonical}", want ${want}`);
	need(file, text, `<link rel="alternate" type="text/markdown" href="${SITE_URL}${page.markdown}"`);
	if (page.path !== '/') read(page.markdown.slice(1));
}

const robots = read('robots.txt');
need('robots.txt', robots, `Sitemap: ${SITE_URL}/sitemap.xml`);
need('robots.txt', robots, 'Allow: /');
const sitemap = read('sitemap.xml');
for (const page of sitePages()) if (page.updated) need('sitemap.xml', sitemap, `<lastmod>${page.updated}`);
if (/<loc>(?!https:\/\/)/.test(sitemap)) failures.push('sitemap.xml has a relative URL');
const security = read('.well-known/security.txt');
need('security.txt', security, 'Contact: ');
need('security.txt', security, 'Expires: ');

if (failures.length) {
	console.error(`check-site: failed\n  ${failures.join('\n  ')}`);
	process.exit(1);
}
console.log(`check-site: ${sitePages().length} pages, index.md and llms.txt in step with the page, site files present`);
