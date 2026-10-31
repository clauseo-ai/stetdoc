/**
 * The parts of a docs page around the article body: breadcrumbs, the page
 * actions, the page foot, the end mark and the pager. "On this page" lives
 * in the shell, once per band: the dock on S and M, a header key on L, a
 * column from XL.
 */
import type { Doc, NavPage } from '../../lib/docs';
import { StetDots } from '../stet-mark';
import { Icon } from './icons';
import { DOCS_UI, OPEN_IN, openInPrompt, shortDate } from './strings';

/** pages whose markdown is over 60 KB fetch the twin on Copy page rather
 * than carry it in the HTML (the changelog, at 61 KB) */
export const INLINE_MARKDOWN_MAX = 60_000;

/** the trail above the H1, without the page itself, which the H1 names */
export function Breadcrumbs({ trail }: { trail: Doc['breadcrumbs'] }) {
	const above = trail.slice(0, -1);
	if (above.length === 0) return null;
	return (
		<nav
			className='crumbs'
			aria-label={DOCS_UI.breadcrumbs}>
			<ol>
				{above.map((c) => (
					<li key={c.name}>{c.path ? <a href={c.path}>{c.name}</a> : <span>{c.name}</span>}</li>
				))}
			</ol>
		</nav>
	);
}

export function PageActions({ doc }: { doc: Doc }) {
	const inline = doc.markdownBytes <= INLINE_MARKDOWN_MAX;
	const prompt = encodeURIComponent(openInPrompt(doc.markdownPath));
	return (
		<div
			className='actions'
			role='group'
			aria-label={DOCS_UI.pageActions}>
			<button
				type='button'
				className='act act-copy'
				aria-label={DOCS_UI.copyPageLabel}
				data-copy-page={doc.markdownPath}>
				<span className='copy-idle'>
					<Icon name='copy' />
					<span className='act-label'>{DOCS_UI.copyPage}</span>
				</span>
				<span className='copy-done'>
					<Icon name='check' />
					<span className='act-label'>{DOCS_UI.copied}</span>
				</span>
			</button>
			<span
				className='sr-only'
				role='status'
			/>
			<a
				className='act'
				href={doc.markdownPath}>
				<Icon name='markdown' />
				<span className='act-label'>{DOCS_UI.viewMarkdown}</span>
			</a>
			<button
				type='button'
				className='act act-open'
				popoverTarget='open-in'
				aria-label={DOCS_UI.openInLabel}>
				<Icon name='open' />
				<span className='act-label'>{DOCS_UI.openIn}</span>
				<span
					className='chev'
					aria-hidden='true'
				/>
			</button>
			<div
				id='open-in'
				popover='auto'
				className='open-in'
				role='dialog'
				aria-label={DOCS_UI.openInLabel}>
				<p className='open-in-note'>{DOCS_UI.openInNote}</p>
				<ul>
					{OPEN_IN.map((app) => (
						<li key={app.name}>
							<a
								href={app.base + prompt}
								target='_blank'
								rel='noopener'>
								<Icon
									name={app.name}
									className='app-mark'
								/>
								<span className='app-name'>{app.name}</span>
								<Icon
									name='external'
									className='app-go'
								/>
							</a>
						</li>
					))}
				</ul>
				<button
					type='button'
					className='open-in-close'
					popoverTarget='open-in'
					popoverTargetAction='hide'>
					{DOCS_UI.close}
				</button>
			</div>
			{inline ? <template id='page-md'>{doc.markdown}</template> : null}
		</div>
	);
}

export function PageFoot({ doc }: { doc: Doc }) {
	return (
		<footer className='doc-foot'>
			{doc.updated ? (
				<p>
					{DOCS_UI.updated} <time dateTime={doc.updated}>{shortDate(doc.updated)}</time>
				</p>
			) : null}
			<p>
				<a href={doc.edit.href}>{doc.edit.label}</a>
			</p>
			<p>
				{DOCS_UI.somethingWrong} <a href={doc.issueHref}>{DOCS_UI.openIssue}</a>
			</p>
		</footer>
	);
}

/** the stet mark's three dots: the page ends standing */
export function EndMark() {
	return (
		<div
			className='end-mark'
			aria-hidden='true'>
			<StetDots />
		</div>
	);
}

function PagerLink({ page, dir }: { page: NavPage; dir: 'prev' | 'next' }) {
	return (
		<a
			className={`pg pg-${dir}`}
			href={page.path}
			rel={dir}>
			<span className='pg-dir'>{dir === 'prev' ? DOCS_UI.previous : DOCS_UI.next}</span>
			<span className='pg-title'>{page.navLabel}</span>
			<span className='pg-sec'>{page.sectionTitle}</span>
		</a>
	);
}

export function Pager({ prev, next }: { prev?: NavPage; next?: NavPage }) {
	if (!prev && !next) return null;
	return (
		<nav
			className='pager'
			aria-label={DOCS_UI.pager}>
			{prev ? (
				<PagerLink
					page={prev}
					dir='prev'
				/>
			) : (
				<span />
			)}
			{next ? (
				<PagerLink
					page={next}
					dir='next'
				/>
			) : null}
		</nav>
	);
}
