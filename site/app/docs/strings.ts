/**
 * Every word the docs chrome says: header, menu, search, page actions, page
 * foot, pager and the 404. Written to COPY.md's voice: plain words, sentence
 * case, straight quotes, no dashes, no parentheses, no mid-sentence colons.
 * The article text itself comes from the repo's markdown (lib/docs).
 */
import { SITE_URL } from '../links';

export const DOCS_UI = {
	skip: 'Skip to the page',
	brand: 'stetdoc',
	brandHome: 'stetdoc home',
	docs: 'docs',
	docsHome: 'Docs home',
	github: 'GitHub',
	githubMark: 'stetdoc on GitHub',
	headerNav: 'Docs',
	version: (v: string) => `Version ${v}. Read the changelog`,

	/* navigation */
	sidebar: 'Docs pages',
	menu: 'Menu',
	onThisPage: 'On this page',
	close: 'Close',
	dock: 'Docs navigation',
	breadcrumbs: 'Breadcrumb',
	backToTop: 'Back to the top',

	/* search */
	search: 'Search',
	searchDocs: 'Search the docs',
	searchShortcut: 'Press / to search',
	searchResults: 'Results',
	searchNeedsJs: 'Search needs JavaScript, which is off in this browser. Every page is in the menu.',
	searchOpenMenu: 'Open the menu',
	searchLoading: 'Loading the search index',
	searchFailed: "The search index didn't load. Check the connection and try again.",
	searchEmpty: (q: string) => `Nothing in the docs matches ${q}.`,
	searchCount: (n: number) => (n === 1 ? '1 result' : `${n} results`),
	searchHint: 'Try a command, a flag or an error code, such as replace, --json or ambiguous_match.',

	/* page actions */
	pageActions: 'Page actions',
	copyPage: 'Copy page',
	copyPageLabel: 'Copy page as markdown',
	copied: 'Copied',
	copyFailed: "Couldn't copy. Open View as markdown instead.",
	viewMarkdown: 'View as markdown',
	openIn: 'Open in',
	openInLabel: 'Open in an AI app',
	openInNote: 'Opens the app with a prompt to read this page.',

	/* page foot */
	updated: 'Updated',
	somethingWrong: 'Something wrong on this page?',
	openIssue: 'Open an issue',
	pager: 'More pages',
	previous: 'Previous',
	next: 'Next',

	/* 404 */
	notFoundTitle: 'Page not found · stetdoc',
	notFoundHead: 'Stet.',
	/* two sentences, each kept on one line */
	notFoundLine: ["That page isn't here.", 'The docs are.'],
	notFoundHome: 'Looking for the launch page?',
	notFoundHomeLink: 'Go to stetdoc.clauseo.chat',
	notFoundPages: 'Every page of the docs',
} as const;

/** the prompt for the Open in links */
export const openInPrompt = (markdownPath: string) =>
	`Read ${SITE_URL}${markdownPath} and help me use stetdoc.`;

/**
 * AI apps that take a prefilled prompt in the URL, in alphabetical order.
 * Checked in a browser on 2026-10-31. Microsoft Copilot is left out: its
 * ?q= is dropped by a redirect to the home page. Gemini has no prefill URL.
 * Each gets its mark at the same size, in this order: no favourite.
 */
export const OPEN_IN: readonly { name: string; base: string }[] = [
	{ name: 'ChatGPT', base: 'https://chatgpt.com/?q=' },
	{ name: 'Claude', base: 'https://claude.ai/new?q=' },
	{ name: 'Grok', base: 'https://grok.com/?q=' },
	{ name: 'Le Chat', base: 'https://chat.mistral.ai/chat?q=' },
	{ name: 'Perplexity', base: 'https://www.perplexity.ai/search?q=' },
];

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** "31 Oct 2026" from an ISO date, in the date's own timezone */
export function shortDate(iso: string) {
	const [y, m, d] = iso.slice(0, 10).split('-').map(Number);
	return `${d} ${MONTHS[m - 1]} ${y}`;
}
