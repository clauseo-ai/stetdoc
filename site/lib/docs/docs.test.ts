import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import type { Element, ElementContent, Root } from 'hast';
import { consoleBlock, codeFigure } from './code';
import { docMetadata, getDoc } from './index';
import { bindUnits, flagSpans, renderMarkdown } from './render';
import { fillPlaceholders, headingIds, LinkError, parseMarkdown, readSource, REPO_ROOT, resolveLink } from './source';
import { twinMarkdown } from './twin';

/* ---------- helpers ---------- */

const SITE = 'https://stetdoc.clauseo.chat';
const all = (node: Root | Element, tag: string): Element[] =>
	node.children.flatMap((c) => (c.type === 'element' ? [...(c.tagName === tag ? [c] : []), ...all(c, tag)] : []));
const textOf = (node: ElementContent | Root): string =>
	node.type === 'text' ? node.value : 'children' in node ? node.children.map((c) => textOf(c as ElementContent)).join('') : '';
const classes = (el: Element) => (el.properties.className as string[] | undefined) ?? [];
const render = (md: string, source = 'docs/README.md', options = {}) => renderMarkdown(`# Title\n\n${md}`, source, options);

/** GitHub's heading ids, from the raw lines: fence-aware, its slug rule, duplicates numbered */
function githubIds(file: string): string[] {
	const seen = new Map<string, number>();
	const ids: string[] = [];
	let fence: string | null = null;
	for (const line of readFileSync(join(REPO_ROOT, file), 'utf8').split('\n')) {
		const f = line.match(/^\s*(`{3,}|~{3,})/);
		if (f) {
			if (fence === null) fence = f[1][0];
			else if (f[1][0] === fence) fence = null;
			continue;
		}
		if (fence !== null) continue;
		const h = line.match(/^#{1,6} (.+?)\s*#*\s*$/);
		if (!h) continue;
		const text = h[1].replace(/`/g, '').replace(/\[([^\]]*)\]\([^)]*\)/g, '$1').replace(/\*\*|__/g, '');
		const base = text.toLowerCase().replace(/[^\p{L}\p{N}\p{M}\p{Pc}\- ]/gu, '').replace(/ /g, '-');
		const n = seen.get(base) ?? 0;
		seen.set(base, n + 1);
		ids.push(n ? `${base}-${n}` : base);
	}
	return ids;
}

/* ---------- links ---------- */

describe('links', () => {
	test('a page in the table goes to its /docs URL, with its anchor', () => {
		expect(resolveLink('agents.md', 'docs/README.md')).toEqual({ html: '/docs/agents', twin: `${SITE}/docs/agents.md` });
		expect(resolveLink('install.md#installing-without-a-terminal-prompt', 'docs/README.md').html).toBe(
			'/docs/install#installing-without-a-terminal-prompt',
		);
		expect(resolveLink('../FIDELITY.md', 'docs/README.md').html).toBe('/docs/fidelity');
		expect(resolveLink('README.md', 'docs/quickstart.md')).toEqual({ html: '/docs', twin: `${SITE}/docs.md` });
	});

	test('a repo file not on the site goes to GitHub, a folder to /tree/', () => {
		expect(resolveLink('../ARCHITECTURE.md', 'docs/README.md').html).toBe('https://github.com/clauseo-ai/stetdoc/blob/main/ARCHITECTURE.md');
		expect(resolveLink('decisions/0013-mcp-for-fewer-turns.md', 'docs/README.md').twin).toBe(
			'https://github.com/clauseo-ai/stetdoc/blob/main/docs/decisions/0013-mcp-for-fewer-turns.md',
		);
		expect(resolveLink('../fuzz/', 'docs/README.md').html).toBe('https://github.com/clauseo-ai/stetdoc/tree/main/fuzz');
	});

	test('an absolute GitHub link to a page on the site goes to the site', () => {
		expect(resolveLink('https://github.com/clauseo-ai/stetdoc/blob/main/docs/agents.md#the-author', 'README.md')).toEqual({
			html: '/docs/agents#the-author',
			twin: `${SITE}/docs/agents.md#the-author`,
		});
		const other = 'https://github.com/clauseo-ai/stetdoc/blob/main/ARCHITECTURE.md';
		expect(resolveLink(other, 'README.md').html).toBe(other);
		expect(resolveLink('https://example.com/x', 'README.md').html).toBe('https://example.com/x');
	});

	test('the build fails on a missing file or a missing anchor', () => {
		expect(() => resolveLink('nope.md', 'docs/README.md')).toThrow(LinkError);
		expect(() => resolveLink('agents.md#no-such-heading', 'docs/README.md')).toThrow(/no heading with the id "no-such-heading"/);
		expect(() => render('[x](nope.md)')).toThrow(LinkError);
		expect(() => render('[x](agents.md#nope)')).toThrow(LinkError);
	});

	test('rendered links: internal plain, external marked for screen readers', () => {
		const { tree } = render('[a](agents.md) [b](../ARCHITECTURE.md) [c](https://example.com)');
		const [a, b, c] = all(tree, 'a');
		expect(a.properties.href).toBe('/docs/agents');
		expect(a.properties.dataExternal).toBeUndefined();
		expect(b.properties.dataExternal).toBe('github');
		expect(textOf(b)).toBe('b (on GitHub)');
		expect(c.properties.dataExternal).toBe('site');
		expect(textOf(c)).toBe('c (opens another site)');
	});
});

/* ---------- headings and the TOC ---------- */

describe('headings', () => {
	for (const file of ['docs/agents.md', 'docs/reference/cli.md']) {
		test(`ids match GitHub's for every heading in ${file}`, () => {
			const ours = headingIds(parseMarkdown(readFileSync(join(REPO_ROOT, file), 'utf8'))).map((h) => h.id);
			expect(ours).toEqual(githubIds(file));
		});
	}

	test("cli.md numbers its repeated Examples as GitHub does, and a '# ' in a fence is code", () => {
		const ids = headingIds(parseMarkdown(readSource('docs/reference/cli.md'))).map((h) => h.id);
		const examples = ids.filter((id) => /^examples(-\d+)?$/.test(id));
		expect(examples).toEqual(['examples', ...Array.from({ length: 13 }, (_, i) => `examples-${i + 1}`)]);
		expect(ids).not.toContain('stetdoc-editing-word-documents-with-tracked-changes');
		expect(ids).toContain('stetdoc-unpack');
	});

	test('the agent guide keeps the anchor the binary prints', () => {
		expect(headingIds(parseMarkdown(readSource('docs/agents.md'))).map((h) => h.id)).toContain('the-claude-code-guard-hook');
	});

	test('h2 to h4 link to themselves; the H1 stays out of the body', () => {
		const { tree, title } = render('## Two\n\n### Three\n\n#### Four\n\n##### Five');
		expect(title).toBe('Title');
		expect(all(tree, 'h1')).toHaveLength(0);
		const h2 = all(tree, 'h2')[0];
		expect(h2.properties.id).toBe('two');
		const a = h2.children[0] as Element;
		expect(classes(a)).toEqual(['anchor']);
		expect(a.properties.href).toBe('#two');
		expect(all(all(tree, 'h5')[0], 'a')).toHaveLength(0);
	});

	test('the TOC is h2 and h3, without h3 Examples; the changelog lists h2 only', () => {
		const md = '## Alpha\n\n### Examples\n\n### Beta\n\n#### Gamma\n\n## Delta';
		expect(render(md).toc).toEqual([
			{ id: 'alpha', text: 'Alpha', depth: 2 },
			{ id: 'beta', text: 'Beta', depth: 3 },
			{ id: 'delta', text: 'Delta', depth: 2 },
		]);
		expect(render(md, 'docs/README.md', { tocH2Only: true }).toc.map((t) => t.id)).toEqual(['alpha', 'delta']);
		const cli = getDoc('reference/cli').toc;
		expect(cli.some((t) => t.text === 'Examples')).toBe(false);
		expect(cli.find((t) => t.id === 'stetdoc-view')?.depth).toBe(2);
		expect(getDoc('changelog').toc.every((t) => t.depth === 2)).toBe(true);
	});
});

/* ---------- code ---------- */

describe('code', () => {
	test("console blocks copy only commands; heredoc bodies go with their command", () => {
		const src = readFileSync(join(REPO_ROOT, 'README.md'), 'utf8');
		const block = src.slice(src.indexOf("stetdoc edit contract.stet <<'EOF'"));
		const heredoc = block.slice(0, block.indexOf('\nEOF\n') + 4);
		const { lines, copy } = consoleBlock(`$ ${heredoc}\nreceipt line\nerror[stale_target]: no\nnext: run it again`);
		expect(copy).toBe(heredoc);
		expect(copy).toContain('paragraphs: <<END');
		const kinds = lines.map((l) => classes(l).join(' '));
		expect(kinds[0]).toBe('ln cmd');
		expect(kinds.slice(1, heredoc.split('\n').length)).toEqual(Array(heredoc.split('\n').length - 1).fill('ln cmd cont'));
		expect(kinds.slice(-3)).toEqual(['ln out', 'ln err', 'ln next']);
		const prompt = lines[0].children[0] as Element;
		expect(classes(prompt)).toEqual(['prompt']);
		expect(prompt.properties.ariaHidden).toBe('true');
	});

	test("cli.md's examples copy their commands without $", () => {
		const md = readSource('docs/reference/cli.md');
		const blocks = [...md.matchAll(/```console\n([\s\S]*?)```/g)].map((m) => m[1]);
		expect(blocks.length).toBeGreaterThan(10);
		for (const b of blocks) {
			const { copy } = consoleBlock(b.trimEnd());
			const commands = b.split('\n').filter((l) => l.startsWith('$ ')).map((l) => l.slice(2));
			for (const c of commands) expect(copy.split('\n')).toContain(c);
			expect(copy).not.toMatch(/^\$ /m);
		}
		expect(consoleBlock('$ stetdoc skills get core\n# stetdoc: editing\n...').copy).toBe('stetdoc skills get core');
	});

	test('the figure: head, copy button, status span next to it, trimmed lines', () => {
		const fig = codeFigure('console', '$ stetdoc --version\nstetdoc 0.9.0\n\n\n');
		expect(fig.properties).toMatchObject({ className: ['code'], dataKind: 'console', dataLang: 'console' });
		const head = fig.children[0] as Element;
		expect(all(head, 'span').some((s) => classes(s).includes('code-lang'))).toBe(false);
		const button = head.children[0] as Element;
		expect(button.properties).toMatchObject({ ariaLabel: 'Copy the commands', dataCopy: 'stetdoc --version' });
		expect((head.children[1] as Element).properties).toMatchObject({ className: ['sr-only'], role: 'status' });
		expect(all(fig, 'span').filter((s) => classes(s).includes('ln'))).toHaveLength(2);
	});

	test('source blocks: token classes, no styles, the language label', () => {
		const fig = codeFigure('json', '{"name": "stetdoc", "n": 3, "ok": true}');
		const head = fig.children[0] as Element;
		expect(textOf(head.children[0] as Element)).toBe('json');
		expect(((head.children[1] as Element).properties.ariaLabel)).toBe('Copy the code');
		const tokens = all(fig, 'span').flatMap(classes).filter((c) => c.startsWith('t-'));
		expect(tokens).toContain('t-property');
		expect(tokens).toContain('t-constant');
		expect(JSON.stringify(fig)).not.toMatch(/style|var\(--/);
		expect(codeFigure('Caddyfile', 'example.com {\n}').properties.dataKind).toBe('text');
		expect(codeFigure(undefined, 'plain').properties.dataKind).toBe('text');
	});
});

/* ---------- blocks ---------- */

describe('blocks', () => {
	test('GitHub alerts become labelled callouts', () => {
		const { tree } = render('> [!TIP]\n> Use find.\n\n> [!CAUTION]\n> Careful.\n\n> plain quote');
		const asides = all(tree, 'aside');
		expect(asides.map((a) => [a.properties.dataKind, textOf(a.children.find((c) => c.type === 'element') as Element)])).toEqual([
			['note', 'Tip'],
			['warning', 'Caution'],
		]);
		expect(asides[0].properties.role).toBe('note');
		expect(textOf(asides[0])).toContain('Use find.');
		expect(textOf(asides[0])).not.toContain('[!TIP]');
		expect(all(tree, 'blockquote')).toHaveLength(1);
	});

	test('tables: a labelled region, data-label per cell, data-keyed, alignment as data', () => {
		const { tree } = render('## Codes\n\n| Code | Exit |\n|---|--:|\n| `a` | 1 |\n| `b` | 2 |\n\n| Name | Note |\n|---|---|\n| `a` and more | x |');
		const [first, second] = all(tree, 'div').filter((d) => classes(d).includes('table'));
		expect(first.properties).toMatchObject({ role: 'region', tabIndex: 0, ariaLabel: 'Codes' });
		const table = first.children[0] as Element;
		expect(table.properties.dataKeyed).toBe('');
		const tds = all(table, 'td');
		expect(tds.map((td) => td.properties.dataLabel)).toEqual(['Code', 'Exit', 'Code', 'Exit']);
		expect(tds[1].properties.dataAlign).toBe('right');
		expect(tds[1].properties.align).toBeUndefined();
		expect((second.children[0] as Element).properties.dataKeyed).toBeUndefined();
	});

	test('tables carry explicit ARIA roles for the phone cards', () => {
		const { tree } = render('| Code | Exit |\n|---|---|\n| `a` | 1 |');
		const table = all(tree, 'table')[0];
		expect(table.properties.role).toBe('table');
		expect([...all(table, 'thead'), ...all(table, 'tbody')].map((g) => g.properties.role)).toEqual(['rowgroup', 'rowgroup']);
		expect(all(table, 'tr').map((r) => r.properties.role)).toEqual(['row', 'row']);
		expect(all(table, 'th').map((c) => c.properties.role)).toEqual(['columnheader', 'columnheader']);
		expect(all(table, 'td').map((c) => c.properties.role)).toEqual(['cell', 'cell']);
	});

	test('key-value lists', () => {
		const { tree } = render('- **Skill:** a file\n- **MCP:** a line\n\nBetween.\n\n- **Byte identity.** not a key\n- **Other:** x');
		const [kv, plain] = all(tree, 'ul');
		expect(classes(kv)).toEqual(['kv']);
		const key = all(kv, 'strong')[0];
		expect(classes(key)).toEqual(['kv-key']);
		expect(textOf(key)).toBe('Skill:');
		expect((key.children[0] as { value: string }).value).toBe('Skill');
		expect(classes(key.children[1] as Element)).toEqual(['sr-only']);
		expect(classes(plain)).toEqual([]);
		expect(getDoc('agents').markdown).toContain('- **Docs:**');
	});

	test('the lead, and the overview doors', () => {
		const { tree } = render('First.\n\n## Start here\n\n**One.** a\n\n**Two.** b\n\nNot a door.\n\n## Next');
		expect(classes(tree.children[0] as Element)).toEqual(['lead']);
		expect(all(tree, 'div').some((d) => classes(d).includes('doors'))).toBe(false);
		const doors = render('First.\n\n## Start here\n\n**One.** a\n\n**Two.** b\n\nNot a door.\n\n## Next', 'docs/README.md', { doors: true }).tree;
		const group = all(doors, 'div').find((d) => classes(d).includes('doors'))!;
		expect(group.children.map((c) => classes(c as Element))).toEqual([['door'], ['door']]);
		const overview = render(readSource('docs/README.md').replace(/^# .*\n/, ''), 'docs/README.md', { doors: true }).tree;
		expect(all(overview, 'div').filter((d) => classes(d).includes('door'))).toHaveLength(3);
	});

	test('raw HTML is shown as text', () => {
		const { tree } = render('Type <INPUT> here.\n\n<div>block</div>');
		expect(all(tree, 'div').filter((d) => !classes(d).length)).toHaveLength(0);
		expect(textOf(tree)).toContain('<INPUT>');
		expect(textOf(tree)).toContain('<div>block</div>');
	});
});

/* ---------- review follow-ups ---------- */

describe('reading on phones', () => {
	test('inline code over 28 characters is class "long"; shorter code stays plain', () => {
		const { tree } = render('Run `--force` or `stetdoc view contract.docx --find "x"`.');
		const codes = all(tree, 'code');
		expect(classes(codes[0])).toEqual([]);
		expect(classes(codes[1])).toEqual(['long']);
		// words stay whole, spaces break, the text is unchanged
		expect(textOf(codes[1])).toBe('stetdoc view contract.docx --find "x"');
		const parts = codes[1].children.map((c) => (c.type === 'element' ? `[${textOf(c)}]` : (c as { value: string }).value));
		expect(parts).toEqual(['[stetdoc]', ' ', '[view]', ' ', '[contract.docx]', ' ', '[--find]', ' ', '["x"]']);
		const url = 'https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/x.sh';
		const long = all(render(`Run \`curl ${url}\`.`).tree, 'code')[0];
		// a URL over 28 characters breaks only after a slash, never inside "//"
		const pieces = long.children.map((c) => (c.type === 'element' ? (c.tagName === 'wbr' ? '|' : `[${textOf(c)}]`) : (c as { value: string }).value));
		expect(pieces).toEqual(['[curl]', ' ', '[https://]', '|', '[raw.githubusercontent.com/]', '|', '[clauseo-ai/]', '|', '[stetdoc/]', '|', '[main/]', '|', '[x.sh]']);
		expect(textOf(long)).toBe(`curl ${url}`);
	});

	test('glossary entries: the bold term is the key, the Avoid note its own span, the text unchanged', () => {
		const md = '**Package.** The `.docx` file.\n*Avoid:* archive, zip.\n\nA plain paragraph with **bold** inside.';
		const { tree } = render(md, 'docs/README.md', { glossary: true });
		const ps = all(tree, 'p');
		expect(classes(ps[0])).toEqual(['gl-entry']);
		expect(classes(all(ps[0], 'strong')[0])).toEqual(['gl-term']);
		const avoid = all(ps[0], 'span').find((s) => classes(s).includes('gl-avoid'))!;
		expect(textOf(avoid)).toBe('Avoid: archive, zip.');
		expect(textOf(ps[0])).toBe('Package. The .docx file.\nAvoid: archive, zip.');
		expect(classes(ps[1])).toEqual([]);
	});

	test('a --flag in running text is held on one line; code and longer words are left alone', () => {
		const { tree } = render('Batches come from stdin or --ops, and re-runs take `--json`.');
		const spans = all(tree, 'span').filter((s) => classes(s).includes('nobr'));
		expect(spans.map((s) => textOf(s))).toEqual(['--ops']);
		expect(textOf(tree)).toContain('Batches come from stdin or --ops, and re-runs take --json.');
		expect(flagSpans('a --b-c d').map((c) => (c.type === 'element' ? `[${textOf(c)}]` : (c as { value: string }).value))).toEqual(['a ', '[--b-c]', ' d']);
	});

	test('sh, bash, shell and zsh fences are data-kind="shell", tokenised, copied whole, a scrolling region', () => {
		for (const lang of ['sh', 'bash', 'shell', 'zsh']) {
			const fig = codeFigure(lang, 'brew install clauseo-ai/tap/stetdoc\necho "done"');
			expect(fig.properties.dataKind).toBe('shell');
			expect(all(fig, 'pre')[0].properties).toMatchObject({ role: 'region', tabIndex: 0, ariaLabel: `${lang} code` });
			const button = all(fig, 'button')[0];
			expect(button.properties.dataCopy).toBe('brew install clauseo-ai/tap/stetdoc\necho "done"');
			expect(all(fig, 'span').filter((s) => classes(s).includes('ln'))).toHaveLength(2);
			expect(all(fig, 'span').some((s) => classes(s).some((c) => c.startsWith('t-')))).toBe(true);
		}
		expect(codeFigure('sh', '$ stetdoc --version').properties.dataKind).toBe('console');
	});

	test('only source and shell blocks scroll: their pre is a labelled, focusable region', () => {
		const pre = (fig: Element) => all(fig, 'pre')[0].properties;
		expect(pre(codeFigure('json', '{}'))).toEqual({ role: 'region', tabIndex: 0, ariaLabel: 'json code' });
		expect(pre(codeFigure('sh', 'a'))).toEqual({ role: 'region', tabIndex: 0, ariaLabel: 'sh code' });
		for (const fig of [codeFigure('console', '$ a'), codeFigure('text', 'a'), codeFigure(undefined, 'a')])
			expect(pre(fig)).toEqual({});
	});

	test('a text block with box drawing is a diagram: no copy button, no status', () => {
		const diagram = codeFigure('text', 'cloud app ──HTTPS──▶ bridge');
		expect(all(diagram, 'button')).toHaveLength(0);
		expect(all(diagram, 'span').some((s) => s.properties.role === 'status')).toBe(false);
		expect(all(codeFigure(undefined, '┌─┐'), 'button')).toHaveLength(0);
		expect(all(codeFigure('text', 'plain words'), 'button')).toHaveLength(1);
	});

	test('a number and its unit stay together in prose, never in code', () => {
		expect(bindUnits('about 5 ms, 2 s, 40 KB, 1.5 MB, 3 GB, 44 px, 100 %')).toBe(
			'about 5\u00a0ms, 2\u00a0s, 40\u00a0KB, 1.5\u00a0MB, 3\u00a0GB, 44\u00a0px, 100\u00a0%',
		);
		expect(bindUnits('in 5 steps, 3 sections')).toBe('in 5 steps, 3 sections');
		const { tree } = render('It takes 5 ms. Run `sleep 5 s`.');
		expect(textOf(all(tree, 'p')[0])).toContain('5\u00a0ms');
		expect(textOf(all(tree, 'code')[0])).toBe('sleep 5 s');
		expect(getDoc('install').markdown).not.toContain('\u00a0');
	});

	test('the lead class only on a first paragraph of 240 characters or less', () => {
		expect(classes(render('Short opening.').tree.children[0] as Element)).toEqual(['lead']);
		expect(classes(render(`${'word '.repeat(60)}end.`).tree.children[0] as Element)).toEqual([]);
	});

	test("the overview's title isn't doubled", () => {
		expect(docMetadata(getDoc('')).title).toEqual({ absolute: 'stetdoc docs' });
		expect(docMetadata(getDoc('install')).title).toEqual({ absolute: 'Install · stetdoc docs' });
	});
});

/* ---------- the skill and the twins ---------- */

describe('twins', () => {
	test('{{version}} and the skill tables are filled; a leftover placeholder fails', () => {
		const skill = readSource('crates/stetdoc-cli/skill/core.md');
		expect(skill).toContain('stetdoc 0.9.0 turns a .docx');
		expect(skill).not.toMatch(/\{\{/);
		expect(skill).toContain('| `stetdoc unpack` |');
		expect(skill).toMatch(/\| `replace` \| yes \| /);
		expect(() => fillPlaceholders('a {{nope}} b')).toThrow(/unfilled placeholder/);
	});

	test('links absolute, comments gone, the header under the H1', () => {
		const md = '# Page\n\nSee [install](install.md#check-the-install), [arch](../ARCHITECTURE.md), [here](#page) and <https://github.com/clauseo-ai/stetdoc/blob/main/docs/agents.md>.\n\n<!-- a note -->\n\n[ref]: agents.md\n\nEnd.';
		const twin = twinMarkdown(md, 'docs/README.md', {
			description: 'What it is.',
			url: `${SITE}/docs/x`,
			updated: '2026-10-30T02:48:55+05:30',
			version: '0.9.0',
		});
		expect(twin).toBe(
			`# Page\n\n> What it is.\n\nSource: ${SITE}/docs/x · Updated 2026-10-30 · stetdoc 0.9.0\n\nSee [install](${SITE}/docs/install.md#check-the-install), [arch](https://github.com/clauseo-ai/stetdoc/blob/main/ARCHITECTURE.md), [here](#page) and <${SITE}/docs/agents.md>.\n\n[ref]: ${SITE}/docs/agents.md\n\nEnd.\n`,
		);
		const undated = twinMarkdown('# P\n\nx', 'docs/README.md', { description: 'd', url: `${SITE}/docs`, version: '0.9.0' });
		expect(undated).toBe(`# P\n\n> d\n\nSource: ${SITE}/docs · stetdoc 0.9.0\n\nx\n`);
	});

	test("each page's twin has no relative links left, and Doc.markdown is the twin", () => {
		for (const slug of ['', 'agents', 'fidelity', 'node', 'skill']) {
			const doc = getDoc(slug);
			expect(doc.markdown).not.toMatch(/\]\((?!https?:|#|mailto:)[^)]+\)/);
			expect(doc.markdown.split('\n')[4]).toStartWith(`Source: ${SITE}${doc.path}`);
			expect(doc.markdownBytes).toBe(Buffer.byteLength(doc.markdown));
		}
	});
});
