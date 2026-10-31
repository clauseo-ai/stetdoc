/**
 * The docs API the pages import (the contract's section 1). Everything runs
 * at build time on the server: pages read the repo markdown, render it to
 * React elements, and nothing from here ships to the browser.
 */
import { toJsxRuntime } from 'hast-util-to-jsx-runtime';
import type { Metadata } from 'next';
import type { ReactNode } from 'react';
import { Fragment, jsx, jsxs } from 'react/jsx-runtime';
import { SITE_URL } from '../../app/links';
import { pageMetadata } from '../meta';
import { renderMarkdown, type TocEntry } from './render';
import { GITHUB, pagePath, readSource, repoVersion, sourceTitle, twinPath } from './source';
import { DOCS_TITLE_SUFFIX, EDIT, ISSUE_TITLE, OG } from './strings';
import { EXTERNAL_ROWS, PAGES, SECTIONS, type PageEntry, type SectionId } from './table';
import { pageTwin } from './twin';

export type { SectionId, TocEntry };

export type NavPage = {
	/** '' for the overview, 'quickstart', 'reference/cli' */
	slug: string;
	/** '/docs', '/docs/quickstart', '/docs/reference/cli' */
	path: string;
	/** '/docs.md', '/docs/quickstart.md', '/docs/reference/cli.md' */
	markdownPath: string;
	/** the H1 text from the markdown */
	title: string;
	/** the sidebar label */
	navLabel: string;
	section: SectionId;
	sectionTitle: string;
};

export type Section = { id: SectionId; title: string; pages: NavPage[] };

/** sidebar rows that leave the site, e.g. 'Contributing, on GitHub' */
export type ExternalRow = { label: string; href: string; section: SectionId };

export type Doc = NavPage & {
	/** meta description, at most 155 characters */
	description: string;
	/** repo path, 'docs/agents.md' */
	source: string;
	edit: { href: string; label: string };
	/** GitHub new-issue URL with the page URL in the title */
	issueHref: string;
	/** ISO 8601 with timezone, from git; undefined when unknown */
	updated?: string;
	/** first commit of the source, same rules as updated */
	published?: string;
	/** '0.9.0', from Cargo.toml */
	version: string;
	/** the twin's exact text, byte for byte what /docs/x.md serves */
	markdown: string;
	markdownBytes: number;
	/** h2 + h3, filtered (no h3 'Examples'; changelog h2 only) */
	toc: TocEntry[];
	/** the article after the H1 */
	body: ReactNode;
	prev?: NavPage;
	next?: NavPage;
	/** a section has no page: its path is '' and it renders as text */
	breadcrumbs: { name: string; path: string }[];
};

const sectionTitle = (id: SectionId) => SECTIONS.find((s) => s.id === id)!.title;

function navPage(page: PageEntry): NavPage {
	return {
		slug: page.slug,
		path: pagePath(page.slug),
		markdownPath: twinPath(page.slug),
		title: sourceTitle(page.source),
		navLabel: page.navLabel,
		section: page.section,
		sectionTitle: sectionTitle(page.section),
	};
}

let pagesCache: NavPage[] | undefined;
/** every page, flat, in sidebar order */
export function allPages(): NavPage[] {
	pagesCache ??= PAGES.map(navPage);
	return pagesCache;
}

/** the sections in sidebar order; external rows come from externalRows() */
export function sections(): Section[] {
	return SECTIONS.map((s) => ({ ...s, pages: allPages().filter((p) => p.section === s.id) }));
}

export function externalRows(): ExternalRow[] {
	return EXTERNAL_ROWS.map((r) => ({ ...r }));
}

export function docVersion(): string {
	return repoVersion();
}

/** for generateStaticParams of app/docs/[[...slug]]; the overview is { slug: [] } */
export function docParams(): { slug?: string[] }[] {
	return PAGES.map((p) => ({ slug: p.slug ? p.slug.split('/') : [] }));
}

/** the edit link: the source, or for a generated page the source it is generated from */
function editLink(source: string): { href: string; label: string } {
	switch (source) {
		case 'docs/reference/cli.md':
		case 'docs/reference/mcp-tools.md':
			return { href: `${GITHUB}/tree/main/crates/stetdoc-cli/doc`, label: EDIT.generatedCli };
		case 'docs/reference/operations.md':
		case 'docs/reference/errors.md':
		case 'docs/reference/findings.md': {
			const file = { o: 'op_spec.rs', e: 'error.rs', f: 'check.rs' }[source.split('/').pop()![0] as 'o' | 'e' | 'f'];
			return { href: `${GITHUB}/blob/main/crates/stetdoc-core/src/api/${file}`, label: EDIT.generatedFrom(file) };
		}
		default:
			return { href: `${GITHUB}/edit/main/${source}`, label: EDIT.page };
	}
}

const docCache = new Map<string, Doc>();

/** one page, rendered; throws on an unknown slug */
export function getDoc(slug: string): Doc {
	const hit = docCache.get(slug);
	if (hit) return hit;
	const index = PAGES.findIndex((p) => p.slug === slug);
	if (index < 0) throw new Error(`docs: no page with the slug "${slug}"`);
	const page = PAGES[index];
	const nav = allPages()[index];
	const twin = pageTwin(page);
	const { tree, toc, title } = renderMarkdown(readSource(page.source), page.source, {
		doors: page.slug === '',
		tocH2Only: page.slug === 'changelog',
		glossary: page.slug === 'glossary',
	});
	if (title !== nav.title) throw new Error(`docs: ${page.source}'s H1 is "${title}", its first "# " line says "${nav.title}"`);
	const url = `${SITE_URL}${nav.path}`;
	const doc: Doc = {
		...nav,
		description: page.description,
		source: page.source,
		edit: editLink(page.source),
		issueHref: `${GITHUB}/issues/new?title=${encodeURIComponent(ISSUE_TITLE(url))}`,
		updated: twin.updated,
		published: twin.published,
		version: repoVersion(),
		markdown: twin.markdown,
		markdownBytes: Buffer.byteLength(twin.markdown, 'utf8'),
		toc,
		body: toJsxRuntime(tree, { Fragment, jsx, jsxs }),
		prev: index > 0 ? allPages()[index - 1] : undefined,
		next: index < PAGES.length - 1 ? allPages()[index + 1] : undefined,
		breadcrumbs:
			page.slug === ''
				? [{ name: 'Docs', path: '/docs' }]
				: [
						{ name: 'Docs', path: '/docs' },
						{ name: nav.sectionTitle, path: '' },
						{ name: nav.navLabel, path: nav.path },
					],
	};
	docCache.set(slug, doc);
	return doc;
}

/** the OG card's path for a page: /docs/og/<slug or 'index'>.png */
export const ogPath = (slug: string) => `/docs/og/${slug || 'index'}.png`;

/** title, description, canonical, markdown alternate and the page's own card */
export function docMetadata(doc: Doc): Metadata {
	// the overview's H1 is already "stetdoc docs"
	const title = doc.slug === '' ? doc.title : `${doc.title}${DOCS_TITLE_SUFFIX}`;
	const base = pageMetadata({ path: doc.path, markdown: doc.markdownPath, title, description: doc.description });
	const card = { url: ogPath(doc.slug), width: 1200, height: 630, alt: OG.alt(doc.title), type: 'image/png' };
	return {
		...base,
		openGraph: {
			...base.openGraph,
			type: 'article',
			...(doc.updated ? { modifiedTime: doc.updated } : {}),
			...(doc.published ? { publishedTime: doc.published } : {}),
			images: [card],
		},
		twitter: { ...base.twitter, images: [card] },
	};
}

const ORG = 'https://www.clauseo.chat/#org';

/** the page's JSON-LD @graph: Article + TechArticle, the breadcrumb trail */
export function docJsonLd(doc: Doc): object {
	const url = `${SITE_URL}${doc.path}`;
	const trail = [
		{ name: 'stetdoc', item: `${SITE_URL}/` },
		{ name: 'Docs', item: `${SITE_URL}/docs` },
		...(doc.slug ? [{ name: doc.navLabel, item: url }] : []),
	];
	return {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': ['Article', 'TechArticle'],
				'@id': `${url}#article`,
				headline: doc.title,
				description: doc.description,
				url,
				mainEntityOfPage: url,
				inLanguage: 'en',
				image: `${SITE_URL}${ogPath(doc.slug)}`,
				...(doc.updated ? { dateModified: doc.updated } : {}),
				...(doc.published ? { datePublished: doc.published } : {}),
				author: { '@id': ORG },
				publisher: { '@id': ORG },
				isPartOf: { '@id': `${SITE_URL}/#website` },
				...(doc.slug === '' ? { about: { '@id': `${SITE_URL}/#app` } } : {}),
			},
			{
				'@type': 'BreadcrumbList',
				'@id': `${url}#breadcrumb`,
				itemListElement: trail.map((t, i) => ({ '@type': 'ListItem', position: i + 1, name: t.name, item: t.item })),
			},
			{ '@type': 'Organization', '@id': ORG, name: 'Clauseo', url: 'https://www.clauseo.chat/' },
		],
	};
}

/** JSON-LD as the text of a <script type="application/ld+json">, with < escaped */
export const jsonLdText = (ld: object) => JSON.stringify(ld).replace(/</g, '\\u003c');
