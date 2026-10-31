/**
 * Code blocks, built at build time into final markup (the contract's
 * section 2): a figure with a head (language label, copy button, status
 * span) and a pre of line spans. Console blocks split commands from output
 * and copy only the commands; source blocks are tokenised by shiki with a
 * css-variables theme, and each token's colour variable becomes a class, so
 * no inline style reaches the page and no shiki code reaches the browser.
 */
import type { Element, ElementContent } from 'hast';
import { createCssVariablesTheme, createHighlighterCoreSync, type HighlighterCore } from 'shiki/core';
import { createJavaScriptRegexEngine } from 'shiki/engine/javascript';
import docker from 'shiki/langs/docker.mjs';
import javascript from 'shiki/langs/javascript.mjs';
import json from 'shiki/langs/json.mjs';
import jsonc from 'shiki/langs/jsonc.mjs';
import python from 'shiki/langs/python.mjs';
import rust from 'shiki/langs/rust.mjs';
import shellscript from 'shiki/langs/shellscript.mjs';
import toml from 'shiki/langs/toml.mjs';
import typescript from 'shiki/langs/typescript.mjs';
import yaml from 'shiki/langs/yaml.mjs';
import { UI } from '../../content/copy';
import { CODE } from './strings';

/** fence info → shiki grammar */
const GRAMMAR: Record<string, string> = {
	sh: 'shellscript',
	bash: 'shellscript',
	shell: 'shellscript',
	zsh: 'shellscript',
	shellscript: 'shellscript',
	json: 'json',
	jsonc: 'jsonc',
	yaml: 'yaml',
	yml: 'yaml',
	toml: 'toml',
	rust: 'rust',
	rs: 'rust',
	ts: 'typescript',
	typescript: 'typescript',
	js: 'javascript',
	javascript: 'javascript',
	mjs: 'javascript',
	python: 'python',
	py: 'python',
	dockerfile: 'docker',
	docker: 'docker',
};
export type CodeKind = 'console' | 'shell' | 'source' | 'text';
/** box drawing and arrowheads: a text block holding them is a diagram */
const DIAGRAM = /[\u2500-\u257F▶◀▲▼]/;
const CONSOLE = new Set(['console', 'shell-session', 'terminal']);
const PLAIN = new Set(['', 'text', 'txt', 'plain', 'plaintext']);

/** colour variable → token class; anything else is plain text */
const CLASS: Record<string, string> = {
	'token-keyword': 't-keyword',
	'token-string': 't-string',
	'token-string-expression': 't-string',
	'token-link': 't-string',
	'token-comment': 't-comment',
	'token-constant': 't-constant',
	'token-function': 't-function',
	'token-parameter': 't-param',
	'token-punctuation': 't-punct',
	'token-type': 't-type',
	'token-property': 't-property',
};

let highlighter: HighlighterCore | undefined;
function shiki(): HighlighterCore {
	if (highlighter) return highlighter;
	const theme = createCssVariablesTheme({ name: 'stet', variablePrefix: '--t-' });
	// the css-variables theme has no type or property tokens; add them
	theme.tokenColors = [
		...(theme.tokenColors ?? []),
		{
			scope: ['entity.name.type', 'entity.name.class', 'support.type.primitive', 'support.class', 'storage.type.primitive', 'entity.name.type.struct'],
			settings: { foreground: 'var(--t-token-type)' },
		},
		{
			scope: [
				'support.type.property-name',
				'support.type.property-name.json',
				'meta.object-literal.key',
				'variable.other.property',
				'variable.other.object.property',
				'entity.name.tag.yaml',
				'meta.mapping.key',
				'support.type.property-name.toml',
				'entity.other.attribute-name',
			],
			settings: { foreground: 'var(--t-token-property)' },
		},
	];
	highlighter = createHighlighterCoreSync({
		themes: [theme],
		langs: [docker, javascript, json, jsonc, python, rust, shellscript, toml, typescript, yaml],
		engine: createJavaScriptRegexEngine({ forgiving: true }),
	});
	return highlighter;
}

const text = (value: string): ElementContent => ({ type: 'text', value });
const el = (tagName: string, properties: Element['properties'], children: ElementContent[]): Element => ({
	type: 'element',
	tagName,
	properties,
	children,
});

/** one line span; every line but the last carries its newline, so lines
    read right with or without CSS and whether .ln is inline or block */
const line = (className: string[], children: ElementContent[], last: boolean): Element =>
	el('span', { className }, last ? children : [...children, text('\n')]);

/* ---------- console ---------- */

/** stetdoc's redline and verdict in its output, marked with the court's
    meanings as the launch page marks them: an insertion is a tracked change
    waiting for review (yellow), a deletion is struck, and "no problems" is a
    check that passed (green). The text stays exactly as stetdoc prints it. */
const MARK = /(\{--.*?--\}|\{\+\+.*?\+\+\}|\bno problems\b)/;
function marks(row: string): ElementContent[] {
	return row
		.split(MARK)
		.filter((piece) => piece !== '')
		.map((piece) =>
			piece.startsWith('{--')
				? el('span', { className: ['rl-del'] }, [text(piece)])
				: piece.startsWith('{++')
					? el('span', { className: ['rl-ins'] }, [text(piece)])
					: piece === 'no problems'
						? el('span', { className: ['rl-ok'] }, [text(piece)])
						: hyphened(piece),
		)
		.flat();
}

/** a flag or a name with a hyphen (--in-place, contract.edited.docx stays
    whole too) never splits at the hyphen on a narrow screen; a long run
    such as a URL or a token may still break */
function hyphened(piece: string): ElementContent[] {
	return piece
		.split(/(\S*-\S*)/)
		.filter((part) => part !== '')
		.map((part) =>
			part.includes('-') && !/\s/.test(part) && part.length <= 28
				? el('span', { className: ['nw'] }, [text(part)])
				: text(part),
		);
}

const HEREDOC = /<<-?\s*(['"]?)([A-Za-z_][A-Za-z0-9_]*)\1/;

/** console lines and the commands to copy, without their prompts */
export function consoleBlock(code: string): { lines: Element[]; copy: string } {
	const rows = code.split('\n');
	const lines: Element[] = [];
	const copy: string[] = [];
	let heredoc: string | null = null;
	let continued = false;
	rows.forEach((row, i) => {
		const last = i === rows.length - 1;
		if (heredoc !== null || continued) {
			lines.push(line(['ln', 'cmd', 'cont'], [text(row)], last));
			copy.push(row);
			if (heredoc !== null) {
				if (row.trim() === heredoc) heredoc = null;
			} else {
				continued = row.endsWith('\\');
				heredoc = row.match(HEREDOC)?.[2] ?? null;
			}
			return;
		}
		if (row.startsWith('$ ') || row === '$') {
			const cmd = row.slice(2);
			lines.push(
				line(
					['ln', 'cmd'],
					[el('span', { className: ['prompt'], ariaHidden: 'true' }, [text('$ ')]), ...(cmd ? marks(cmd) : [])],
					last,
				),
			);
			copy.push(cmd);
			continued = cmd.endsWith('\\');
			heredoc = cmd.match(HEREDOC)?.[2] ?? null;
			return;
		}
		const kind = /^error\[[a-z_]+\]/.test(row) ? 'err' : /^\s*next:/.test(row) ? 'next' : 'out';
		lines.push(line(['ln', kind], row ? marks(row) : [], last));
	});
	return { lines, copy: copy.join('\n') };
}

/* ---------- source ---------- */

function sourceLines(code: string, grammar: string): Element[] {
	const { tokens } = shiki().codeToTokens(code, { lang: grammar, theme: 'stet' });
	return tokens.map((row, i) => {
		const children: ElementContent[] = [];
		let run: { cls: string; value: string } | null = null;
		const flush = () => {
			if (!run) return;
			children.push(run.cls === 't-plain' ? text(run.value) : el('span', { className: [run.cls] }, [text(run.value)]));
			run = null;
		};
		for (const token of row) {
			const variable = token.color?.match(/^var\(--t-(.+)\)$/)?.[1] ?? '';
			const cls = /^\s+$/.test(token.content) ? 't-plain' : (CLASS[variable] ?? 't-plain');
			if (run && run.cls === cls) run.value += token.content;
			else {
				flush();
				run = { cls, value: token.content };
			}
		}
		flush();
		return line(['ln'], children, i === tokens.length - 1);
	});
}

/* ---------- the figure ---------- */

/** the figure for a fenced block; lang is the fence's info word */
export function codeFigure(lang: string | undefined, raw: string): Element {
	const code = raw.replace(/\s+$/, '');
	const info = (lang ?? '').trim();
	const key = info.toLowerCase();
	const shellish = GRAMMAR[key] === 'shellscript' && /^\$ /m.test(code);
	// a shell script (data-kind="shell") is copied and pasted whole, so like
	// source it keeps each line whole and scrolls; console and text wrap
	const kind: CodeKind =
		CONSOLE.has(key) || shellish ? 'console' : GRAMMAR[key] === 'shellscript' ? 'shell' : GRAMMAR[key] ? 'source' : 'text';

	let lines: Element[];
	let copy = code;
	if (kind === 'console') {
		const block = consoleBlock(code);
		lines = block.lines;
		if (block.copy.trim()) copy = block.copy;
	} else if (kind === 'source' || kind === 'shell') {
		lines = sourceLines(code, GRAMMAR[key]);
	} else {
		const rows = code.split('\n');
		lines = rows.map((row, i) => line(['ln'], row ? [text(row)] : [], i === rows.length - 1));
	}

	const label = kind === 'console' || PLAIN.has(key) ? null : info;
	const head: ElementContent[] = [];
	if (label) head.push(el('span', { className: ['code-lang'] }, [text(label)]));
	// a diagram is for reading, not copying
	const diagram = kind === 'text' && DIAGRAM.test(code);
	if (!diagram) head.push(
		el(
			'button',
			{
				type: 'button',
				className: ['copy'],
				ariaLabel: kind === 'console' ? CODE.copyCommands : CODE.copyCode,
				dataCopy: copy,
			},
			[el('span', { className: ['copy-idle'] }, [text(UI.copy)]), el('span', { className: ['copy-done'] }, [text(UI.copied)])],
		),
		el('span', { className: ['sr-only'], role: 'status' }, []),
	);
	const properties: Element['properties'] = { className: ['code'], dataKind: kind };
	if (info && !PLAIN.has(key)) properties.dataLang = key;
	// only source and shell blocks scroll, so only they are a focusable,
	// labelled region
	const pre: Element['properties'] =
		kind === 'source' || kind === 'shell' ? { role: 'region', tabIndex: 0, ariaLabel: CODE.source(info) } : {};
	return el('figure', properties, [
		...(head.length ? [el('div', { className: ['code-head'] }, head)] : []),
		el('pre', pre, [el('code', {}, lines)]),
	]);
}
