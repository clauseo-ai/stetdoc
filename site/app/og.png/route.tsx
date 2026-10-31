import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { ImageResponse } from 'next/og';

/*
 * The card a shared link unfurls with, drawn at build: the h1 with its stet
 * dots and the stet mark on the concrete ground. Satori cannot read a
 * variable woff2, so og/ holds a static TTF instance of Anek Latin at width
 * 125% and weight 600 (fontTools varLib.instancer, latin subset).
 *
 * A route named og.png rather than the opengraph-image convention, so the
 * static export writes a file with an extension and any static host serves
 * it as image/png. layout.tsx points openGraph and twitter at it.
 */
export const dynamic = 'force-static';
const size = { width: 1200, height: 630 };

const INK = '#23201D';
const GROUND = '#E5E2DC';
const MUTED = '#48423B';

function Dots({ d }: { d: number }) {
	// the mark's dots: diameter 8, centres 12 apart, 32 wide
	return (
		<div style={{ display: 'flex', gap: d / 2, marginTop: d * 0.6 }}>
			{[0, 1, 2].map((i) => (
				<div
					key={i}
					style={{
						width: d,
						height: d,
						borderRadius: d,
						background: INK,
					}}
				/>
			))}
		</div>
	);
}

export async function GET() {
	const anek = await readFile(
		join(process.cwd(), 'og/AnekLatin-wdth125-wght600.ttf'),
	);
	const H1 = 150;
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
					<div style={{ fontSize: 46, letterSpacing: '-0.015em' }}>
						stetdoc
					</div>
				</div>
				<div style={{ display: 'flex', flexDirection: 'column' }}>
					<div
						style={{
							display: 'flex',
							alignItems: 'flex-start',
							fontSize: H1,
							lineHeight: 1,
							letterSpacing: '-0.035em',
						}}>
						<div style={{ marginRight: H1 * 0.24 }}>Let it</div>
						<div
							style={{
								display: 'flex',
								flexDirection: 'column',
								alignItems: 'center',
							}}>
							<div>stand</div>
							<Dots d={Math.round(H1 * 0.13)} />
						</div>
						<div>.</div>
					</div>
					<div
						style={{
							marginTop: 40,
							fontSize: 32,
							color: MUTED,
							letterSpacing: '-0.01em',
						}}>
						stetdoc is how AI agents work in Word.
					</div>
				</div>
			</div>
		),
		{
			...size,
			fonts: [{ name: 'Anek', data: anek, weight: 600, style: 'normal' }],
		},
	);
}
