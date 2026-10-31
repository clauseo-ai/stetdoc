/**
 * The docs shell, shared by every docs page and the site's 404: the header,
 * the sidebar from L (1024px), the page, the TOC column from XL (1440px),
 * the footer, the dock on S and M (under 1024px), the sheets and the two
 * inline scripts. The bands are set out in docs.css.
 * docs.css is imported here so the 404, which lives outside app/docs,
 * gets it too.
 */
import type * as React from 'react';
import { docVersion, type TocEntry } from '../../lib/docs';
import { FOOTER, SITE, UI } from '../../content/copy';
import { COPY_SCRIPT } from '../copy-button';
import { GitHubMark } from '../github-mark';
import { GITHUB_URL } from '../links';
import { Lockup } from '../lockup';
import { Dock, Header, MenuSheet, NavList, Rail, SearchSheet, TocList, TocSheet } from './chrome';
import { DOCS_SCRIPT } from './script';
import { DOCS_UI } from './strings';
import './docs.css';

/** the TOC shows on pages with at least three entries */
export const hasToc = (toc?: TocEntry[]) => (toc?.length ?? 0) >= 3;

export function DocsShell({
	current,
	toc,
	side = true,
	children,
}: {
	current?: string;
	toc?: TocEntry[];
	/** the sidebar from L; the 404 lists the pages in its own body */
	side?: boolean;
	children: React.ReactNode;
}) {
	const withToc = hasToc(toc);
	const grid = ['docs', withToc ? 'with-toc' : '', side ? '' : 'no-side'].filter(Boolean).join(' ');
	return (
		<div className='dx'>
			<a
				className='skip'
				href='#main'>
				{DOCS_UI.skip}
			</a>
			<Header
				version={docVersion()}
				hasToc={withToc}
			/>
			<div className={grid}>
				{side ? (
					<nav
						className='side dnav'
						aria-label={DOCS_UI.sidebar}>
						<NavList
							current={current}
							idp='side'
						/>
					</nav>
				) : null}
				<main
					id='main'
					className='doc-main'
					tabIndex={-1}>
					{children}
				</main>
				{withToc && toc ? (
					<nav
						className='toc-col'
						aria-labelledby='toc-col-head'>
						<p
							className='toc-head'
							id='toc-col-head'>
							{DOCS_UI.onThisPage}
						</p>
						<TocList toc={toc} />
					</nav>
				) : null}
			</div>
			<footer className='foot'>
				<p className='from'>
					{FOOTER.from}
					<a
						className='lockup-link'
						href={SITE.links.clauseo}>
						<Lockup />
					</a>
				</p>
				<nav aria-label={UI.footNavLabel}>
					<a href='/'>{SITE.name}</a>
					<a
						className='gh'
						href={GITHUB_URL}
						aria-label={DOCS_UI.githubMark}>
						<GitHubMark />
					</a>
					<a href='/docs/changelog'>{FOOTER.changelog}</a>
					<a href='/llms.txt'>{FOOTER.llms}</a>
					<a href='/sitemap.xml'>{FOOTER.sitemap}</a>
				</nav>
			</footer>
			<Dock hasToc={withToc} />
			<Rail hasToc={withToc} />
			<MenuSheet current={current} />
			{withToc && toc ? <TocSheet toc={toc} /> : null}
			<SearchSheet />
			<script
				id='copy-script'
				dangerouslySetInnerHTML={{ __html: COPY_SCRIPT }}
			/>
			<script
				data-keep=''
				dangerouslySetInnerHTML={{ __html: DOCS_SCRIPT }}
			/>
		</div>
	);
}
