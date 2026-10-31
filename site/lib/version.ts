import { readFileSync } from 'node:fs';
import { join } from 'node:path';

/** the version this site last shipped with, if ../Cargo.toml can't be read */
const PINNED = '0.9.0';

/**
 * stetdoc's version, from the workspace Cargo.toml one folder up. Vercel
 * builds with the repo checked out, so it is there; if the project ever
 * excludes files outside site/, the pinned version is used and the build
 * says so.
 */
export function stetdocVersion(): string {
	try {
		const toml = readFileSync(join(process.cwd(), '..', 'Cargo.toml'), 'utf8');
		const match = toml.match(/\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/);
		if (match) return match[1];
	} catch {}
	console.warn(`stetdoc version: ../Cargo.toml not readable, using ${PINNED}`);
	return PINNED;
}
