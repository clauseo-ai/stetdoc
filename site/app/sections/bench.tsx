import type * as React from 'react';
import { PARTS } from '../../content/copy';
import { Body } from './primitives';

/* a wall of tiles, every one lit court green, as Clauseo lights a paper the
   answer cites; the figure beside it carries the count in words */
function Wall({ tiles }: { tiles: number }) {
	return (
		<span
			className='wall'
			aria-hidden='true'>
			{Array.from({ length: tiles }, (_, i) => (
				<i key={i} />
			))}
		</span>
	);
}

/* the benchmark is the mark at full size: a full-bleed stet bar carries the
   heading and the race, head to head, stetdoc's time in full ink against
   the other setups' range in a quieter one; each track runs from 0 to the
   row's slowest setup, labelled at both ends. The three facts hang under
   the bar on the mark's three dots. */
export function Bench() {
	const p = PARTS.bench;
	const c = p.chart;
	return (
		<section
			className='part part-bench'
			aria-labelledby={p.id}>
			<div className='bench-bar'>
				<h2 id={p.id}>{p.title}</h2>
				<div className='part-body'>
					<Body paras={p.body} />
				</div>
				<div className='race'>
					<table
						className='race-table'
						role='table'>
						<caption className='race-caption'>
							{c.caption}
							<span
								className='race-key'
								aria-hidden='true'>
								<span className='key key-stet'>{c.keyStet}</span>
								<span className='key key-others'>{c.keyOthers}</span>
							</span>
						</caption>
						<thead className='sr-only'>
							<tr role='row'>
								{c.columns.map((col) => (
									<th
										key={col}
										scope='col'
										role='columnheader'>
										{col}
									</th>
								))}
							</tr>
						</thead>
						<tbody role='rowgroup'>
							{c.rows.map((row) => {
								const [fast, slow] = row.others;
								const style = {
									'--stet': row.stetdoc / slow,
									'--from': fast / slow,
								} as React.CSSProperties;
								return (
									<tr
										key={row.model}
										className='race-row'
										role='row'
										style={style}>
										<th
											scope='row'
											role='rowheader'
											className='race-model'>
											{row.model}
										</th>
										<td
											role='cell'
											className='race-num'>
											<span
												className='race-label'
												aria-hidden='true'>
												{c.keyStet}
											</span>
											<span className='race-fig'>
												{row.stetdoc}
												<span className='race-unit'> s</span>
											</span>
										</td>
										<td
											role='cell'
											className='race-others'>
											<span
												className='race-label'
												aria-hidden='true'>
												{c.othersLabel}
											</span>
											<span className='race-fig'>
												{/* the unit never wraps alone */}
												{c.range(fast, slow).replace(/ (?=\S+$)/, '\u00a0')}
											</span>
										</td>
										<td
											className='race-scale'
											aria-hidden='true'>
											<span className='race-track'>
												<span className='race-bar'>
													<span className='race-bar-label'>{c.keyStet}</span>
												</span>
												<span className='race-band' />
											</span>
											<span className='race-ends'>
												<span>{c.scaleStart}</span>
												<span>{c.scaleEnd(slow)}</span>
											</span>
										</td>
									</tr>
								);
							})}
						</tbody>
					</table>
					<p className='method'>{p.method}</p>
				</div>
			</div>
			<ul className='facts'>
				{p.facts.map((fact) => (
					<li
						className={fact.wall ? `fact fact-wall-${fact.wall}` : 'fact'}
						key={fact.figure}>
						<span
							className='fact-dot'
							aria-hidden='true'
						/>
						<span className='fact-figure'>{fact.figure}</span>{' '}
						{fact.wall ? <Wall tiles={fact.wall} /> : null}
						<span className='fact-line'>{fact.line}</span>
					</li>
				))}
			</ul>
		</section>
	);
}
