/*
 * Writes the docs' markdown twins into public/, before `next build` copies
 * public/ into out/: public/docs.md for the overview and
 * public/docs/<slug>.md for every other page, each the exact text of
 * Doc.markdown (lib/docs/twin.ts). Both paths are gitignored.
 *
 * Usage: bun scripts/docs-artifacts.ts (package.json's build runs it first).
 */
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { DOCS } from '../lib/docs/manifest';

const TWIN_LIMIT = 120 * 1024;
const publicDir = join(import.meta.dir, '..', 'public');
rmSync(join(publicDir, 'docs'), { recursive: true, force: true });
rmSync(join(publicDir, 'docs.md'), { force: true });

const failures: string[] = [];
let bytes = 0;
for (const doc of DOCS) {
	const file = join(publicDir, doc.slug ? `docs/${doc.slug}.md` : 'docs.md');
	const size = Buffer.byteLength(doc.markdown, 'utf8');
	if (size > TWIN_LIMIT) failures.push(`${doc.slug || 'docs'}.md is ${size} bytes, over ${TWIN_LIMIT}`);
	mkdirSync(dirname(file), { recursive: true });
	writeFileSync(file, doc.markdown);
	bytes += size;
}

if (failures.length) {
	console.error(`docs-artifacts: failed\n  ${failures.join('\n  ')}`);
	process.exit(1);
}
console.log(`docs-artifacts: ${DOCS.length} twins, ${Math.round(bytes / 1024)} KB, in public/`);
