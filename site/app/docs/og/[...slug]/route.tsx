import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ImageResponse } from 'next/og';
import { allPages } from '../../../../lib/docs';
import { OG } from '../../../../lib/docs/strings';

/*
 * Each docs page's share card, drawn once at build as a static PNG at
 * /docs/og/<slug or 'index'>.png: the concrete ground, the stet mark with
 * "stetdoc docs" and the section small above, and the page title in a
 * graphite bar, the stet mark's bar, set in the launch card's Anek at 125%
 * width (og/AnekLatin-wdth125-wght600.ttf, the same instance app/og.png
 * uses). Colours are the launch card's.
 */
export const dynamic = 'force-static';
export const dynamicParams = false;
const size = { width: 1200, height: 630 };

const INK = '#23201D';
const GROUND = '#E5E2DC';
const MUTED = '#48423B';

type Params = { slug: string[] };

export function generateStaticParams(): Params[] {
	return allPages().map((p) => {
		const parts = p.slug ? p.slug.split('/') : ['index'];
		parts[parts.length - 1] += '.png';
		return { slug: parts };
	});
}

/** the title's size, so the longest titles still fit the bar in two lines */
const titleSize = (title: string) => (title.length <= 14 ? 112 : title.length <= 24 ? 88 : title.length <= 34 ? 72 : 60);

export async function GET(_req: Request, { params }: { params: Promise<Params> }) {
	const parts = [...(await params).slug];
	parts[parts.length - 1] = parts[parts.length - 1].replace(/\.png$/, '');
	const slug = parts.join('/') === 'index' ? '' : parts.join('/');
	const page = allPages().find((p) => p.slug === slug);
	if (!page) return new Response('Not found', { status: 404 });

	const anek = await readFile(join(process.cwd(), 'og/AnekLatin-wdth125-wght600.ttf'));
	const fontSize = titleSize(page.title);
	return new ImageResponse(
		(
			<div
				style={{
					width: '100%',
					height: '100%',
					display: 'flex',
					flexDirection: 'column',
					justifyContent: 'space-between',
					padding: '76px 84px 92px',
					background: GROUND,
					color: INK,
					fontFamily: 'Anek',
				}}>
				<div style={{ display: 'flex', flexDirection: 'column', gap: 18 }}>
					<div style={{ display: 'flex', alignItems: 'center', gap: 20 }}>
						<svg
							width='52'
							height='36'
							viewBox='0 0 32 22'>
							<rect
								width='32'
								height='8'
								fill={INK}
							/>
							<circle
								cx='4'
								cy='18'
								r='4'
								fill={INK}
							/>
							<circle
								cx='16'
								cy='18'
								r='4'
								fill={INK}
							/>
							<circle
								cx='28'
								cy='18'
								r='4'
								fill={INK}
							/>
						</svg>
						<div style={{ fontSize: 46, letterSpacing: '-0.015em' }}>{OG.brand}</div>
					</div>
					<div style={{ fontSize: 32, color: MUTED, letterSpacing: '-0.01em' }}>{page.sectionTitle}</div>
				</div>
				<div
					style={{
						display: 'flex',
						alignSelf: 'flex-start',
						maxWidth: '100%',
						padding: `${Math.round(fontSize * 0.22)}px ${Math.round(fontSize * 0.36)}px ${Math.round(fontSize * 0.28)}px`,
						background: INK,
						color: GROUND,
						fontSize,
						lineHeight: 1.05,
						letterSpacing: '-0.03em',
					}}>
					{page.title}
				</div>
			</div>
		),
		{
			...size,
			fonts: [{ name: 'Anek', data: anek, weight: 600, style: 'normal' }],
		},
	);
}
