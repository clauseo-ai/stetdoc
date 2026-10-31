/*
 * Removes Next's client runtime from the static export, after `next build`.
 *
 * Every page on this site is static and navigates with ordinary full-page
 * loads, and none has a client component: the copy buttons are one small
 * inline script and the install picker is CSS. Next still sends its router
 * and React (about 134 KB gzipped) plus an inline RSC payload; this step
 * takes them out of out/ so a page ships its HTML, its inlined CSS and its
 * font, and nothing else.
 *
 * It refuses to run, and fails the build, when stripping could break a
 * page: a client component of our own in the client manifest or the
 * source, or a streamed Suspense boundary that needs script to resolve.
 * After stripping it checks that no page lost a word and that no framework
 * script tag is left.
 *
 * Usage: bun scripts/strip-runtime.ts (package.json's build runs it).
 */
import { readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, relative } from 'node:path';

const root = join(import.meta.dir, '..');
const out = join(root, 'out');
const failures: string[] = [];
const fail = (why: string) => failures.push(why);

function walk(dir: string): string[] {
	return readdirSync(dir).flatMap((name) => {
		const path = join(dir, name);
		return statSync(path).isDirectory() ? walk(path) : [path];
	});
}

/* ---------- 1. is it safe? ---------- */

// our own client components: any "use client" module outside node_modules
for (const file of walk(join(root, '.next', 'server', 'app')).filter((f) =>
	f.endsWith('_client-reference-manifest.js'),
)) {
	const text = readFileSync(file, 'utf8');
	const json = text.slice(text.indexOf('{', text.indexOf('] =')));
	const manifest = JSON.parse(json.replace(/;\s*$/, '')) as {
		clientModules: Record<string, unknown>;
	};
	for (const id of Object.keys(manifest.clientModules)) {
		if (!id.startsWith('[project]/node_modules/'))
			fail(`client component ${id} in ${relative(root, file)}`);
	}
}
for (const dir of ['app', 'content']) {
	for (const file of walk(join(root, dir)).filter((f) => /\.(t|j)sx?$/.test(f))) {
		if (/^\s*['"]use client['"]/m.test(readFileSync(file, 'utf8')))
			fail(`"use client" in ${relative(root, file)}`);
	}
}

const pages = walk(out).filter((f) => f.endsWith('.html'));
for (const page of pages) {
	const html = readFileSync(page, 'utf8');
	// a pending Suspense boundary or a streamed segment needs script to show
	if (/<!--\$\?-->|<template id="B:|\$RC\(|\$RS\(/.test(html))
		fail(`streamed content that needs script in ${relative(root, page)}`);
}

if (failures.length) {
	console.error(`strip-runtime: not safe to strip\n  ${failures.join('\n  ')}`);
	process.exit(1);
}

/* ---------- 2. strip ---------- */

/** the words a reader or a crawler gets, in order */
function words(html: string) {
	return html
		.replace(/<script\b[\s\S]*?<\/script>/g, ' ')
		.replace(/<style\b[\s\S]*?<\/style>/g, ' ')
		.replace(/<[^>]+>/g, ' ')
		.replace(/\s+/g, ' ')
		.trim();
}

let removedBytes = 0;
for (const page of pages) {
	const before = readFileSync(page, 'utf8');
	const after = before
		// framework scripts, by src
		.replace(/<script\b[^>]*\bsrc="\/_next\/[^"]*"[^>]*>\s*<\/script>/g, '')
		// inline scripts: the RSC payload and its bootstraps; the copy script,
		// JSON-LD and any <script data-keep> are kept
		.replace(/<script\b(?![^>]*\bid="copy-script")(?![^>]*application\/ld\+json)(?![^>]*\bdata-keep\b)[^>]*>[\s\S]*?<\/script>/g, (tag) =>
			/\bsrc=/.test(tag) ? tag : '',
		)
		// preloads for the scripts just removed
		.replace(/<link\b[^>]*\bas="script"[^>]*\/?>/g, '')
		// the h1's font, first in line
		.replace(
			/<link rel="preload"([^>]*)as="font"([^>]*)\/?>/g,
			(tag) => (tag.includes('fetchPriority') || tag.includes('fetchpriority') ? tag : tag.replace('<link ', '<link fetchpriority="high" ')),
		);

	if (/<script\b[^>]*\/_next\/static\/chunks\//.test(after))
		fail(`a framework script is left in ${relative(root, page)}`);
	if (after.includes('self.__next_f'))
		fail(`the RSC payload is left in ${relative(root, page)}`);
	if (words(before) !== words(after))
		fail(`stripping changed the words of ${relative(root, page)}`);
	if (!/<h1[\s>]/.test(after) && !page.endsWith('404.html') && !page.includes('_not-found'))
		fail(`no h1 in ${relative(root, page)}`);

	removedBytes += before.length - after.length;
	writeFileSync(page, after);
}

const copy = readFileSync(join(out, 'index.html'), 'utf8').match(
	/<script id="copy-script">([\s\S]*?)<\/script>/,
);
if (!copy) fail('the copy script is missing from index.html');
else if (copy[1].length > 1100) fail(`the copy script is ${copy[1].length} bytes`);

// the files only the client runtime fetched: JS chunks and RSC payloads
let removedFiles = 0;
for (const file of walk(out)) {
	const rel = relative(out, file);
	const rsc =
		rel.endsWith('.txt') &&
		(/(^|\/)__next\./.test(rel) || /^\d+:/.test(readFileSync(file, 'utf8').slice(0, 8)));
	if ((rel.startsWith('_next/') && rel.endsWith('.js')) || rsc) {
		rmSync(file);
		removedFiles++;
	}
}

// and the directories that held only those
function prune(dir: string) {
	for (const name of readdirSync(dir)) {
		const path = join(dir, name);
		if (statSync(path).isDirectory()) prune(path);
	}
	if (dir !== out && readdirSync(dir).length === 0) rmSync(dir, { recursive: true });
}
prune(out);

if (failures.length) {
	console.error(`strip-runtime: failed\n  ${failures.join('\n  ')}`);
	process.exit(1);
}
console.log(
	`strip-runtime: ${pages.length} pages, ${Math.round(removedBytes / 1024)} KB of inline payload and tags removed, ${removedFiles} runtime files deleted`,
);
