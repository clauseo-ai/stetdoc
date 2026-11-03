import { Fragment } from 'react';
import { END, NAV } from '../../content/copy';
import { DocLinks, Inline, InstallBar } from './primitives';

/* the end's picker: radio inputs and :checked, no script. Every command is
   in the HTML; CSS shows the chosen one's panel. Homebrew is checked in the
   HTML; lib/os-script.ts checks the visitor's system's channel instead and
   moves its tab first, and CSS shows "Suggested" on that tab alone. */
export function Picker() {
	return (
		<fieldset className='picker'>
			<legend className='picker-label'>{END.pickerLabel}</legend>
			<div className='picker-tabs'>
				{END.channels.map((ch) => (
					<Fragment key={ch.id}>
						<input
							type='radio'
							name='channel'
							id={`ch-${ch.id}`}
							className='picker-input'
							defaultChecked={ch.id === END.channels[0].id}
						/>
						<label
							htmlFor={`ch-${ch.id}`}
							className='picker-tab'>
							{ch.name}
							{ch.os ? (
								<span className={`picker-tag tag-${ch.os}`}> {NAV.suggested}</span>
							) : null}
						</label>
					</Fragment>
				))}
			</div>
			{END.channels.map((ch) => (
				<div
					key={ch.id}
					className={`picker-panel panel-${ch.id}${ch.os ? ` pick-${ch.os}` : ''}`}>
					<InstallBar command={ch.command} />
					<p className='picker-note'>{ch.note}</p>
				</div>
			))}
		</fieldset>
	);
}


/* ---------- the end: the bar again, carrying every way to install ---------- */

export function End() {
	return (
	<section
		className='end'
		aria-labelledby={END.id}>
		<div className='end-bar'>
			<div className='end-head'>
				<h2 id={END.id}>{END.title}</h2>
				<p className='end-intro'>{END.intro}</p>
			</div>
			<div className='end-pick'>
				<Picker />
				<p className='why'>
					<Inline text={END.why} />
				</p>
				<DocLinks />
			</div>
		</div>
		<p className='footnote'>{END.footnote}</p>
	</section>
	);
}
