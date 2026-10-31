/**
 * The docs pages in the shape the site-wide files read: lib/pages.ts for
 * sitemap.xml, lib/markdown.ts for llms.txt and llms-full.txt. The table
 * itself (slugs, sources, sections, descriptions) is lib/docs/table.ts.
 * `updated` is the source file's last commit date from git, or undefined
 * when git can't say for sure; never the build time.
 */
import { PAGES, SECTIONS } from './table';
import { pageTwin } from './twin';

export { EXTERNAL_ROWS, PAGES, SECTIONS, type PageEntry, type SectionId } from './table';

export type DocPage = {
	/** the URL path under /docs, such as 'install'; '' is the overview at /docs */
	slug: string;
	title: string;
	/** one sentence: what the page answers */
	summary: string;
	/** the source's last commit, ISO 8601; undefined when git can't say */
	updated?: string;
	/** the markdown twin's exact text */
	markdown: string;
	sectionTitle: string;
	optional: boolean;
};

export const DOCS: DocPage[] = PAGES.map((page) => {
	const twin = pageTwin(page);
	return {
		slug: page.slug,
		title: twin.title,
		summary: page.description,
		updated: twin.updated,
		markdown: twin.markdown,
		sectionTitle: SECTIONS.find((s) => s.id === page.section)!.title,
		optional: Boolean(page.optional),
	};
});
