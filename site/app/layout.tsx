import type { Metadata, Viewport } from 'next';
import type * as React from 'react';
import { anek, anekExt } from './fonts';
import { SITE } from '../content/copy';
import './globals.css';

/* site-wide defaults only; each page sets its canonical, twin and card
   through lib/meta.ts */
export const metadata: Metadata = {
	metadataBase: new URL(SITE.url),
	title: { absolute: SITE.title },
	description: SITE.description,
};

export const viewport: Viewport = {
	themeColor: [
		{ media: '(prefers-color-scheme: light)', color: '#e5e2dc' },
		{ media: '(prefers-color-scheme: dark)', color: '#1b1a18' },
	],
	colorScheme: 'light dark',
};

export default function RootLayout({
	children,
}: {
	children: React.ReactNode;
}) {
	return (
		<html
			lang='en'
			className={`${anek.variable} ${anekExt.variable}`}>
			<body>{children}</body>
		</html>
	);
}
