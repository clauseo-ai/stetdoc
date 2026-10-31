/*
 * Serves the static build (out/) the way Vercel does, closely enough to
 * measure and to curl: it applies vercel.json's routes (headers, the
 * Accept: text/markdown rewrite, clean URLs, the 404), compresses text and
 * caches hashed assets for good. Usage: bun run serve [port] [host];
 * defaults 3210 and 0.0.0.0. Vercel's own routing is the authority; this
 * is an emulation of it.
 */
import { join, normalize } from 'node:path';

type Route = {
	src?: string;
	dest?: string;
	headers?: Record<string, string>;
	continue?: boolean;
	check?: boolean;
	status?: number;
	handle?: string;
	has?: { type: 'header'; key: string; value: { re: string } | string }[];
};

const root = join(import.meta.dir, '..', 'out');
const config = (await Bun.file(join(import.meta.dir, '..', 'vercel.json')).json()) as {
	routes?: Route[];
};
const routes = config.routes ?? [];
const split = (phase: string) => {
	const i = routes.findIndex((r) => r.handle === phase);
	return i;
};
const fsAt = split('filesystem');
const errAt = split('error');
const before = fsAt < 0 ? routes : routes.slice(0, fsAt);
const after = fsAt < 0 ? [] : routes.slice(fsAt + 1, errAt < 0 ? undefined : errAt);
const onError = errAt < 0 ? [] : routes.slice(errAt + 1);

const port = Number(process.argv[2] ?? 3210);
const hostname = process.argv[3] ?? '0.0.0.0';
const TEXT = /\.(html|css|js|svg|txt|json|xml|md|webmanifest)$/;
const TYPES: Record<string, string> = {
	'.md': 'text/markdown; charset=utf-8',
	'.txt': 'text/plain; charset=utf-8',
	'.html': 'text/html; charset=utf-8',
	'.xml': 'application/xml',
};

async function file(path: string) {
	const clean = normalize(decodeURIComponent(path)).replace(/^(\.\.[/\\])+/, '');
	const candidates = clean.endsWith('/') ? [join(clean, 'index.html')] : [clean];
	for (const candidate of candidates) {
		const f = Bun.file(join(root, candidate));
		if ((await f.exists()) && f.size > 0) return { f, name: candidate };
	}
	return null;
}

const matches = (r: Route, path: string, req: Request) => {
	if (!r.src) return null;
	const m = path.match(new RegExp(r.src));
	if (!m) return null;
	for (const h of r.has ?? []) {
		const got = req.headers.get(h.key) ?? '';
		const re = typeof h.value === 'string' ? h.value : h.value.re;
		if (!new RegExp(re).test(got)) return null;
	}
	return m;
};
const fill = (dest: string, m: RegExpMatchArray) =>
	dest.replace(/\$(\d)/g, (_, n) => m[Number(n)] ?? '');

Bun.serve({
	port,
	hostname,
	async fetch(req) {
		const url = new URL(req.url);
		let path = url.pathname;
		const headers = new Headers();
		let status = 200;
		for (const r of before) {
			const m = matches(r, path, req);
			if (!m) continue;
			for (const [k, v] of Object.entries(r.headers ?? {})) headers.set(k, fill(v, m));
			// a redirect: a status and a Location header, no dest
			if (r.status && !r.dest) return new Response(null, { status: r.status, headers });
			if (r.dest) path = fill(r.dest, m);
			if (!r.continue) break;
		}
		let hit = await file(path === '/' ? '/index.html' : path);
		if (!hit) {
			for (const r of after) {
				const m = matches(r, path, req);
				if (!m || !r.dest) continue;
				const next = fill(r.dest, m);
				const found = await file(next);
				if (found || !r.check) {
					hit = found;
					break;
				}
			}
		}
		if (!hit) {
			for (const r of onError) {
				const m = matches(r, path, req);
				if (!m) continue;
				status = r.status ?? 404;
				hit = r.dest ? await file(r.dest) : null;
				break;
			}
			if (!hit) return new Response('Not found', { status: 404 });
		}
		const ext = hit.name.slice(hit.name.lastIndexOf('.'));
		if (!headers.has('content-type'))
			headers.set('content-type', TYPES[ext] ?? hit.f.type);
		headers.set(
			'cache-control',
			hit.name.startsWith('/_next/static/')
				? 'public, max-age=31536000, immutable'
				: 'public, max-age=0, must-revalidate',
		);
		if (TEXT.test(hit.name) && req.headers.get('accept-encoding')?.includes('gzip')) {
			headers.set('content-encoding', 'gzip');
			headers.append('vary', 'accept-encoding');
			const body = Bun.gzipSync(new Uint8Array(await hit.f.arrayBuffer()), { level: 9 });
			return new Response(req.method === 'HEAD' ? null : body, { status, headers });
		}
		return new Response(req.method === 'HEAD' ? null : hit.f, { status, headers });
	},
});

console.log(`serving out/ on http://${hostname}:${port}`);
