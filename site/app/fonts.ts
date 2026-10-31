import localFont from 'next/font/local';

/*
 * Anek Latin, self-hosted, as on www.clauseo.chat (clauseo-v2 _site/fonts.ts).
 * The latin face is cut down for speed (fontTools varLib.instancer, then
 * pyftsubset): weight 400 to 640 and width 88 to 125%, the ranges this page
 * sets, and the Latin-1 characters plus typographic punctuation. 85 KB in
 * place of 104. latin-ext is the full variable file, never preloaded, so
 * any other character still finds Anek. Licence: OFL (fonts/OFL.txt).
 */
export const anek = localFont({
	src: './fonts/AnekLatin-latin-subset.woff2',
	weight: '400 640',
	style: 'normal',
	display: 'swap',
	variable: '--font-anek',
	adjustFontFallback: false,
	declarations: [
		{ prop: 'font-stretch', value: '88% 125%' },
		{
			prop: 'unicode-range',
			value: 'U+0020-007E, U+00A0-00FF, U+0131, U+0152-0153, U+02C6, U+02DA, U+02DC, U+2013-2014, U+2018-201E, U+2022, U+2026, U+2039-203A, U+20AC, U+2122, U+2212',
		},
	],
});

export const anekExt = localFont({
	src: './fonts/AnekLatin-normal-latin-ext.woff2',
	weight: '100 800',
	style: 'normal',
	display: 'swap',
	variable: '--font-anek-ext',
	adjustFontFallback: false,
	preload: false,
	declarations: [
		{ prop: 'font-stretch', value: '75% 125%' },
		{
			prop: 'unicode-range',
			value: 'U+0100-02BA, U+02BD-02C5, U+02C7-02CC, U+02CE-02D7, U+02DD-02FF, U+0304, U+0308, U+0329, U+1D00-1DBF, U+1E00-1E9F, U+1EF2-1EFF, U+2020, U+20A0-20AB, U+20AD-20C0, U+2113, U+2C60-2C7F, U+A720-A7FF',
		},
	],
});
