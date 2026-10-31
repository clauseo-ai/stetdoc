import type { Metadata } from 'next';
import { SITE, UI } from '../content/copy';

/**
 * One page's metadata: its own canonical, its markdown twin as an
 * alternate, and the share card. Next replaces a layout's openGraph and
 * twitter objects wholesale when a page sets either, so each page sets all
 * of it here. Docs pages call this with their own path, title and summary.
 */
const CARD = {
	url: '/og.png',
	width: 1200,
	height: 630,
	alt: UI.ogAlt,
	type: 'image/png',
};

export function pageMetadata({
	path,
	markdown,
	title,
	description,
}: {
	path: string;
	markdown: string;
	title: string;
	description: string;
}): Metadata {
	return {
		title: { absolute: title },
		description,
		alternates: {
			canonical: path,
			types: { 'text/markdown': markdown },
		},
		openGraph: {
			title,
			description,
			url: path,
			siteName: SITE.name,
			type: 'website',
			locale: 'en',
			images: [CARD],
		},
		twitter: {
			card: 'summary_large_image',
			title,
			description,
			images: [CARD],
		},
	};
}
