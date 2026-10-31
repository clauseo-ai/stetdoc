import type { Metadata } from 'next';
import { NavList, SearchButton } from './docs/chrome';
import { DocsShell } from './docs/shell';
import { DOCS_UI } from './docs/strings';
import { StetMark } from './stet-mark';

/**
 * The site's 404, for every unknown path, the launch page's included. The
 * mark has its one loud moment here, where there's nothing else to read:
 * the full stet over "Stet.", then the way back, by search, by the list of
 * every docs page, or to the launch page for a mistyped address there.
 * Stacked on S and M; from L, and on a phone turned sideways, the mark and
 * "Stet." stand on the left and the way back sits beside them, so the
 * first screen always says the page is missing.
 */
export const metadata: Metadata = {
	title: { absolute: DOCS_UI.notFoundTitle },
	robots: { index: false },
};

export default function NotFound() {
	return (
		<DocsShell side={false}>
			<div className='nf'>
				<div className='nf-id'>
					<StetMark className='nf-mark' />
					<h1 className='nf-head'>{DOCS_UI.notFoundHead}</h1>
				</div>
				<div className='nf-body'>
					<p className='nf-line'>
						<span className='nf-s'>{DOCS_UI.notFoundLine[0]}</span>{' '}
						<span className='nf-s'>{DOCS_UI.notFoundLine[1]}</span>
					</p>
					<SearchButton className='nf-search' />
					<p className='nf-home'>
						{DOCS_UI.notFoundHome} <a href='/'>{DOCS_UI.notFoundHomeLink}</a>
					</p>
					<h2 className='nf-pages-head'>{DOCS_UI.notFoundPages}</h2>
					<nav
						className='nf-pages dnav'
						aria-label={DOCS_UI.notFoundPages}>
						<NavList idp='nf' />
					</nav>
				</div>
			</div>
		</DocsShell>
	);
}
