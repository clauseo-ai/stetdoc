import { DOCS, type DocPage } from './docs/manifest';
import { absolute, sitePages } from './pages';
import { stetdocVersion } from './version';
import {
	AGENT_BLOCK,
	END,
	ENTITY,
	FOOTER,
	HERO,
	type Line,
	NAV,
	PARTS,
	SITE,
} from '../content/copy';
import { AGENT_GUIDE_URL } from '../app/links';

/*
 * The launch page as markdown, from the same content as the HTML
 * (content/copy.ts), for /index.md and /llms-full.txt; and /llms.txt.
 * Inline marks in the copy are markdown already: `code` and **bold**.
 */

// tildes, not backticks: the bundler's minifier mangles a backtick string
// folded into a template literal
const FENCE = '~~~';
const TICK = ['`'].join('');
const inlineCode = (text: string) => [TICK, text, TICK].join('');
const fence = (lang: string, body: string) => `${FENCE}${lang}\n${body}\n${FENCE}`;
const plain = (text: string) => text.replace(/\*\*/g, '');
const terminal = (lines: readonly Line[]) =>
	lines
		.map((l) => (l.kind === 'cmd' ? `$ ${plain(l.text)}` : plain(l.text)))
		.join('\n');

export function homeMarkdown(): string {
	const p = PARTS;
	const s = HERO.sheet;
	const c = p.bench.chart;
	const out: string[] = [];
	out.push(`# ${SITE.title}`, '', `> ${ENTITY}`, '', `## ${HERO.h1}`, '', HERO.sub, '');
	out.push(fence('sh', SITE.install), '', `${NAV.suggested}. Every other channel is under [Install stetdoc](#install-stetdoc). ${SITE.licence}`, '');
	out.push(`Version ${stetdocVersion()}. [${NAV.readDocs}](${absolute(SITE.links.docs)}) · [${NAV.github}](${SITE.links.github})`, '');
	out.push(`### A real run`, '', HERO.caption, '', fence('console', terminal(HERO.run)), '');
	out.push(
		`The file it wrote, ${s.file}, as Word shows it: ${s.heading}. ${s.before}~~${s.deleted}~~<ins>${s.inserted}</ins>${s.after} Balloon: ${s.balloon.author}, ${s.balloon.label} ${s.balloon.text}, ${s.balloon.when}.`,
		'',
	);

	out.push(`## ${p.unreadable.title}`, '', ...p.unreadable.body.flatMap((b) => [b, '']));
	out.push(
		`> **${p.unreadable.dialog.title}**`,
		'>',
		`> ${p.unreadable.dialog.text}`,
		'>',
		`> ${p.unreadable.dialog.buttons.join(' · ')}`,
		'',
		fence('text', plain(p.unreadable.verdict)),
		'',
	);

	out.push(`## ${p.bench.title}`, '', ...p.bench.body.flatMap((b) => [b, '']));
	out.push(`${c.caption}:`, '', `| ${c.columns.join(' | ')} |`, '| --- | --- | --- |');
	for (const row of c.rows)
		out.push(`| ${row.model} | ${row.stetdoc} s | ${row.others[0]} to ${row.others[1]} s |`);
	out.push('', ...p.bench.facts.map((f) => `- **${f.figure}**: ${f.line}`), '', p.bench.method, '');

	out.push(`## ${p.bytes.title}`, '', ...p.bytes.body.flatMap((b) => [b, '']), p.bytes.proof, '');
	out.push(fence('console', `${terminal(p.bytes.run)}\n${p.bytes.verdict}`), '');

	out.push(`## ${p.clauses.title}`, '', ...p.clauses.body.flatMap((b) => [b, '']));
	out.push(
		fence(
			'console',
			[`$ ${p.clauses.command}`, ...p.clauses.hits.map((h) => [h.ref, h.clause, h.text].filter(Boolean).join('  ')), p.clauses.total].join('\n'),
		),
		'',
	);

	out.push(`## ${p.ambiguous.title}`, '', ...p.ambiguous.body.flatMap((b) => [b, '']));
	out.push(
		fence('console', [terminal(p.ambiguous.run), ...p.ambiguous.places.map((x) => `  ${x}`), plain(p.ambiguous.verdict)].join('\n')),
		'',
	);

	out.push(`## ${p.xml.title}`, '', ...p.xml.body.flatMap((b) => [b, '']));
	out.push(fence('console', `$ ${p.xml.attempt}\n${plain(p.xml.verdict)}`), '');

	out.push(`## ${p.agents.title}`, '', ...p.agents.body.flatMap((b) => [b, '']));
	out.push(`### ${AGENT_BLOCK.title}`, '', ENTITY, '', fence('text', AGENT_BLOCK.text), '');

	out.push(`## ${END.title}`, '', END.intro, '', `| Channel | Command | Note |`, '| --- | --- | --- |');
	for (const ch of END.channels)
		out.push(`| ${ch.name}${ch.suggested ? `, ${NAV.suggested.toLowerCase()}` : ''} | ${inlineCode(ch.command.replace(/\|/g, '\\|'))} | ${ch.note} |`);
	out.push('', END.why, '', `[${NAV.readDocs}](${absolute(SITE.links.docs)}) · [${NAV.github}](${SITE.links.github}) · [${FOOTER.changelog}](${SITE.links.changelog})`, '');
	out.push(END.footnote, '', `${FOOTER.from} [Clauseo](${SITE.links.clauseo}).`, '', `Source: ${absolute('/')}`, '');
	return out.join('\n');
}

const docLine = (d: DocPage) =>
	`- [${d.title}](${absolute(d.slug ? `/docs/${d.slug}.md` : '/docs.md')}): ${d.summary}`;

/** one section per docs sidebar section, the home page first; optional pages go under Optional */
function docsSections(home: { title: string; markdown: string; summary: string }): string[] {
	const out: string[] = [];
	const titles = [...new Set(DOCS.map((d) => d.sectionTitle))];
	titles.forEach((title, i) => {
		const docs = DOCS.filter((d) => d.sectionTitle === title && !d.optional);
		if (!docs.length) return;
		out.push(`## ${title}`, '');
		if (i === 0) out.push(`- [${home.title}](${absolute(home.markdown)}): ${home.summary}`);
		out.push(...docs.map(docLine), '');
	});
	return out;
}

export function llmsTxt(): string {
	const pages = sitePages();
	const out = [
		'# stetdoc',
		'',
		`> ${ENTITY}`,
		'',
		HERO.sub,
		'',
		'## Install and connect',
		'',
		fence('text', AGENT_BLOCK.text),
		'',
		'## How agents should use stetdoc',
		'',
		`- Read the [agent guide](${AGENT_GUIDE_URL}) before the first edit.`,
		'- Check the installed version with `stetdoc --version` rather than relying on remembered flags.',
		'- Open, read and edit .docx files only through stetdoc. Never unzip a .docx or rewrite its XML.',
		'',
		...docsSections(pages[0]),
		'## Optional',
		'',
		...DOCS.filter((d) => d.optional).map(docLine),
		`- [Source](${SITE.links.github})`,
		`- [Every page in one file](${absolute('/llms-full.txt')})`,
		'',
	];
	return out.join('\n');
}

export function llmsFullTxt(): string {
	const parts = [homeMarkdown()];
	// each twin opens with its H1 and Source line
	for (const doc of DOCS) if (!doc.optional) parts.push(doc.markdown);
	return parts.join('\n---\n\n');
}

export const markdownResponse = (body: string) =>
	new Response(body, { headers: { 'Content-Type': 'text/markdown; charset=utf-8' } });
export const textResponse = (body: string) =>
	new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
