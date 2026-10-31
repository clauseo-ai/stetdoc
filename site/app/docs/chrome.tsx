/**
 * The docs chrome: header, sidebar and menu list, TOC, dock, sheets and
 * search. All server components; the sheets are HTML popovers, so they open
 * and close with no JavaScript, and Esc and a tap outside close them.
 */
import type * as React from 'react';
import { externalRows, sections, type TocEntry } from '../../lib/docs';
import { GitHubMark } from '../github-mark';
import { Icon } from './icons';
import { GITHUB_URL } from '../links';
import { StetMark } from '../stet-mark';
import { DOCS_UI } from './strings';

/** the sections and their pages; in the sidebar and in the menu sheet */
export function NavList({ current, idp }: { current?: string; idp: string }) {
	const external = externalRows();
	return (
		<>
			{sections().map((s) => (
				<div
					className='nav-sec'
					key={s.id}>
					<p
						className='nav-head'
						id={`${idp}-${s.id}`}>
						{s.title}
					</p>
					<ul aria-labelledby={`${idp}-${s.id}`}>
						{s.pages.map((p) => (
							<li key={p.path}>
								<a
									href={p.path}
									aria-current={p.path === current ? 'page' : undefined}>
									{p.navLabel}
								</a>
							</li>
						))}
						{external
							.filter((e) => e.section === s.id)
							.map((e) => (
								<li key={e.href}>
									<a
										href={e.href}
										data-external='github'>
										{e.label}
									</a>
								</li>
							))}
					</ul>
				</div>
			))}
		</>
	);
}

/** a chrome string with each --flag held on one line, as in the prose */
function withFlags(s: string): React.ReactNode[] {
	return s.split(/(--[a-z][\w-]*)/i).map((part, i) =>
		i % 2 ? (
			<span
				className='nobr'
				key={part}>
				{part}
			</span>
		) : (
			part
		),
	);
}

export function TocList({ toc }: { toc: TocEntry[] }) {
	return (
		<ul className='toc-list'>
			{toc.map((e) => (
				<li
					key={e.id}
					data-depth={e.depth}>
					<a href={`#${e.id}`}>{e.text}</a>
				</li>
			))}
		</ul>
	);
}

export function SearchButton({ className }: { className: string }) {
	return (
		<button
			type='button'
			className={className}
			popoverTarget='search'
			aria-keyshortcuts='/ Meta+K Control+K'>
			<Icon name='search' />
			<span className='search-label'>{DOCS_UI.searchDocs}</span>
			<kbd aria-hidden='true'>/</kbd>
		</button>
	);
}

/** the header; on L it also carries "On this page" for pages with a TOC */
export function Header({ version, hasToc }: { version: string; hasToc: boolean }) {
	return (
		<header className='dh'>
			<div className='dh-in'>
				<p className='dh-id'>
					<a
						className='dh-brand'
						href='/'
						aria-label={DOCS_UI.brandHome}>
						<StetMark className='dh-mark' />
						{DOCS_UI.brand}
					</a>
					<a
						className='dh-docs'
						href='/docs'
						aria-label={DOCS_UI.docsHome}>
						{DOCS_UI.docs}
					</a>
				</p>
				{hasToc ? (
					<button
						type='button'
						className='dh-toc'
						popoverTarget='toc-sheet'>
						<Icon name='toc' />
						{DOCS_UI.onThisPage}
						<span
							className='chev'
							aria-hidden='true'
						/>
					</button>
				) : null}
				<SearchButton className='dh-search' />
				<nav
					className='dh-nav'
					aria-label={DOCS_UI.headerNav}>
					<a
						className='dh-version'
						href='/docs/changelog'
						aria-label={DOCS_UI.version(version)}>
						{version}
					</a>
					<a
						className='gh'
						href={GITHUB_URL}
						aria-label={DOCS_UI.githubMark}>
						<GitHubMark />
					</a>
				</nav>
			</div>
		</header>
	);
}

/** a sheet: rises from the bottom on phones and tablets with Close at the
 * foot, under the thumb; a side sheet with Close in its title row on a
 * phone turned sideways; the TOC sheet hangs under its header key on L */
function Sheet({
	id,
	title,
	children,
}: {
	id: string;
	title: string;
	children: React.ReactNode;
}) {
	return (
		<div
			id={id}
			popover='auto'
			className='dsheet'
			role='dialog'
			aria-label={title}>
			<p
				className='sheet-title'
				aria-hidden='true'>
				{title}
			</p>
			<div className='sheet-body'>{children}</div>
			<div className='sheet-foot'>
				<button
					type='button'
					className='sheet-close'
					popoverTarget={id}
					popoverTargetAction='hide'>
					{DOCS_UI.close}
				</button>
			</div>
		</div>
	);
}

export function MenuSheet({ current }: { current?: string }) {
	return (
		<Sheet
			id='menu-sheet'
			title={DOCS_UI.menu}>
			<div className='dnav'>
				<NavList
					current={current}
					idp='menu'
				/>
			</div>
		</Sheet>
	);
}

export function TocSheet({ toc }: { toc: TocEntry[] }) {
	return (
		<Sheet
			id='toc-sheet'
			title={DOCS_UI.onThisPage}>
			<TocList toc={toc} />
		</Sheet>
	);
}

/** the search sheet: full screen on phones with the field at the top,
 * where the keyboard can't cover it; a panel near the top on wide screens */
export function SearchSheet() {
	return (
		<div
			id='search'
			popover='auto'
			className='dsheet search'
			role='dialog'
			aria-label={DOCS_UI.searchDocs}>
			<div className='search-bar'>
				<div className='search-field'>
					<label
						className='sr-only'
						htmlFor='search-input'>
						{DOCS_UI.searchDocs}
					</label>
					<input
						id='search-input'
						type='search'
						aria-controls='search-results'
						aria-describedby='search-hint'
						enterKeyHint='search'
						autoComplete='off'
						autoCorrect='off'
						autoCapitalize='off'
						spellCheck={false}
						placeholder={DOCS_UI.searchDocs}
					/>
				</div>
				<button
					type='button'
					className='search-close'
					popoverTarget='search'
					popoverTargetAction='hide'>
					{DOCS_UI.close}
				</button>
			</div>
			<div className='search-body'>
				<p
					id='search-status'
					className='search-status'
					role='status'
				/>
				<ul
					id='search-results'
					className='search-results'
					aria-label={DOCS_UI.searchResults}
				/>
				<p
					id='search-hint'
					className='search-hint'>
					{withFlags(DOCS_UI.searchHint)}
				</p>
				<div className='search-off'>
					<p>{DOCS_UI.searchNeedsJs}</p>
					<button
						type='button'
						className='search-menu'
						popoverTarget='menu-sheet'>
						{DOCS_UI.searchOpenMenu}
					</button>
				</div>
			</div>
		</div>
	);
}

export function Dock({ hasToc }: { hasToc: boolean }) {
	return (
		<nav
			className='dock'
			aria-label={DOCS_UI.dock}>
			<button
				type='button'
				popoverTarget='menu-sheet'>
				<Icon name='menu' />
				{DOCS_UI.menu}
			</button>
			{hasToc ? (
				<button
					type='button'
					popoverTarget='toc-sheet'>
					<Icon name='toc' />
					{DOCS_UI.onThisPage}
				</button>
			) : null}
			<button
				type='button'
				popoverTarget='search'>
				<Icon name='search' />
				{DOCS_UI.search}
			</button>
		</nav>
	);
}

/** a phone on its side: once the header has scrolled away, three icon keys
 * stand in the empty margin right of the text, at the bottom, and open the
 * same sheets as the header's keys. Hidden everywhere else. */
export function Rail({ hasToc }: { hasToc: boolean }) {
	return (
		<div
			className='rail'
			role='group'
			aria-label={DOCS_UI.dock}>
			<button
				type='button'
				popoverTarget='menu-sheet'
				aria-label={DOCS_UI.menu}>
				<Icon name='menu' />
			</button>
			{hasToc ? (
				<button
					type='button'
					popoverTarget='toc-sheet'
					aria-label={DOCS_UI.onThisPage}>
					<Icon name='toc' />
				</button>
			) : null}
			<button
				type='button'
				popoverTarget='search'
				aria-label={DOCS_UI.searchDocs}>
				<Icon name='search' />
			</button>
		</div>
	);
}
