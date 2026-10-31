import type { Metadata } from 'next';
import { FOOTER, NAV, SITE, UI } from '../content/copy';
import { pageMetadata } from '../lib/meta';
import { COPY_SCRIPT } from './copy-button';
import { GitHubMark } from './github-mark';
import { Lockup } from './lockup';
import { Bench } from './sections/bench';
import { End } from './sections/end';
import { Hero } from './sections/hero';
import { JsonLd } from './sections/json-ld';
import {
	Agents,
	Ambiguous,
	Bytes,
	Clauses,
	Unreadable,
	Xml,
} from './sections/parts';
import { StetMark } from './stet-mark';
/* the launch page's parts; imported here, not in the root layout, so the
   docs pages don't carry them */
import './parts.css';

/**
 * The launch page, all server components, every word from content/copy.ts.
 * The stet mark is the page's architecture: its bar carries stetdoc's own
 * words (the claim, the install command, each verdict, the benchmark
 * times); its three dots hang under "stand" and hold the three facts.
 * Motion is CSS only (globals.css), played once on load or by the scroll,
 * then still. The sections live in app/sections/. The .launch wrapper
 * makes no box; it carries the page's band tokens (globals.css).
 */

/* each page names its own canonical and markdown twin; the root layout
   names neither, so a docs page never inherits the home page's */
export const metadata: Metadata = pageMetadata({
	path: '/',
	markdown: '/index.md',
	title: SITE.title,
	description: SITE.description,
});

export default function Home() {
	return (
		<div className='launch'>
			<JsonLd />
			<header className='top'>
				<a
					className='brand'
					href='/'>
					<StetMark className='mark' />
					{SITE.name}
				</a>
				<nav
					className='nav'
					aria-label={UI.navLabel}>
					<a href={SITE.links.docs}>{NAV.docs}</a>
					<a
						className='gh'
						href={SITE.links.github}
						aria-label={UI.githubMark}>
						<GitHubMark />
					</a>
				</nav>
			</header>

			<main>
				<Hero />
				<Unreadable />
				<Bench />
				<Bytes />
				<Clauses />
				<Ambiguous />
				<Xml />
				<Agents />

				<End />
			</main>

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
					<a href={SITE.links.docs}>{NAV.docs}</a>
					<a
						className='gh'
						href={SITE.links.github}
						aria-label={UI.githubMark}>
						<GitHubMark />
					</a>
					<a href={SITE.links.changelog}>{FOOTER.changelog}</a>
					<a href='/llms.txt'>{FOOTER.llms}</a>
					<a href='/sitemap.xml'>{FOOTER.sitemap}</a>
				</nav>
			</footer>
			<script
				id='copy-script'
				dangerouslySetInnerHTML={{ __html: COPY_SCRIPT }}
			/>
		</div>
	);
}
