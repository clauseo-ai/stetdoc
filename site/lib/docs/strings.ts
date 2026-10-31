/**
 * The words the docs pipeline puts inside an article: callout labels, the
 * code copy button, the screen-reader notes on outbound links, the page-foot
 * links. The UI's own strings (menu, search, page actions, 404, pager) are in
 * app/docs/strings.ts.
 */
export const CALLOUT = {
	NOTE: { kind: 'note', label: 'Note' },
	TIP: { kind: 'note', label: 'Tip' },
	IMPORTANT: { kind: 'warning', label: 'Important' },
	WARNING: { kind: 'warning', label: 'Warning' },
	CAUTION: { kind: 'warning', label: 'Caution' },
} as const;

export const CODE = {
	copyCode: 'Copy the code',
	copyCommands: 'Copy the commands',
	/** a scrolling source block's accessible name */
	source: (lang: string) => `${lang} code`,
};

export const EXTERNAL = {
	github: ' (on GitHub)',
	site: ' (opens another site)',
};

export const DOCS_TITLE_SUFFIX = ' · stetdoc docs';

export const EDIT = {
	page: 'Edit this page on GitHub',
	generatedCli: "Generated from the binary's help. Edit the source on GitHub",
	/** a reference generated from one Rust file */
	generatedFrom: (file: string) => `Generated from ${file}. Edit the source on GitHub`,
};

export const ISSUE_TITLE = (url: string) => `Docs: ${url}`;

export const OG = {
	brand: 'stetdoc docs',
	alt: (title: string) => `${title}, in the stetdoc docs`,
};
