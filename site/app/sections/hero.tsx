import { END, HERO, NAV, SITE } from '../../content/copy';
import { StetDots } from '../stet-mark';
import { Clause111, Code, DocLinks, InstallBar } from './primitives';

/* ---------- the hero scene: a real run, and the file it wrote ---------- */

export function Scene() {
	const s = HERO.sheet;
	return (
		<figure className='scene'>
			<div className='stage'>
				<div className='sheet'>
					<p className='sheet-bar'>{s.file}</p>
					<div className='sheet-page'>
						<p className='doc-head'>{s.heading}</p>
						<div className='doc-change'>
							<div className='doc-text'>
								<span
									className='change-bar'
									aria-hidden='true'
								/>
								<Clause111 />
							</div>
							<p className='balloon'>
								<span className='balloon-who'>{s.balloon.author}</span>
								<span className='balloon-what'>
									<b>{s.balloon.label}</b> {s.balloon.text}
								</span>
								<span className='balloon-when'>{s.balloon.when}</span>
							</p>
						</div>
						<p className='doc-next'>{s.next}</p>
					</div>
				</div>
				<Code
					className='term'
					lines={HERO.run}
					stagger
				/>
			</div>
			<figcaption>{HERO.caption}</figcaption>
		</figure>
	);
}


/* ---------- the first screen: the mark at the scale of the page ---------- */

export function Hero() {
	return (
	<div className='hero'>
		<h1 className='bar'>
			<span className='bar-in'>
				{HERO.h1.slice(0, HERO.h1.indexOf(HERO.stetWord))}
				<span className='nobr'>
					<span className='stet'>
						{HERO.stetWord}
						<StetDots className='stet-dots' />
					</span>
					{HERO.h1.slice(
						HERO.h1.indexOf(HERO.stetWord) + HERO.stetWord.length,
					)}
				</span>
			</span>
		</h1>
		<div className='hero-row'>
			<p className='sub'>{HERO.sub}</p>
			<div className='cta'>
				<p className='cta-head'>
					<span className='suggested'>{NAV.suggested}</span>
					<a
						className='text-link'
						href={`#${END.id}`}>
						{NAV.otherWays}
					</a>
				</p>
				<InstallBar command={SITE.install} />
				<DocLinks />
				<p className='licence'>{SITE.licence}</p>
			</div>
		</div>
		<Scene />
	</div>

	);
}
