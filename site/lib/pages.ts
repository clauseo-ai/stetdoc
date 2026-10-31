import { SITE_URL } from '../app/links';
import { HERO, SITE } from '../content/copy';
import { DOCS } from './docs/manifest';

/**
 * Every page on the site, for sitemap.xml and llms.txt. The launch page is
 * written here; the docs pages come from lib/docs/manifest.ts.
 */
export type SitePage = {
	/** the path, such as '/' or '/docs/install' */
	path: string;
	/** its markdown twin's path */
	markdown: string;
	title: string;
	summary: string;
	/** last content change, ISO date; undefined when unknown */
	updated?: string;
};

/** the launch page's last content change; bump it when the page's words change */
export const HOME_UPDATED = '2026-10-31';

export function sitePages(): SitePage[] {
	return [
		{
			path: '/',
			markdown: '/index.md',
			title: SITE.title,
			summary: HERO.sub,
			updated: HOME_UPDATED,
		},
		...DOCS.map((doc) => ({
			path: doc.slug ? `/docs/${doc.slug}` : '/docs',
			markdown: doc.slug ? `/docs/${doc.slug}.md` : '/docs.md',
			title: doc.title,
			summary: doc.summary,
			updated: doc.updated,
		})),
	];
}

export const absolute = (path: string) =>
	path === '/' ? `${SITE_URL}/` : `${SITE_URL}${path}`;
