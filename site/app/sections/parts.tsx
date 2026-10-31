import type * as React from 'react';
import { AGENT_BLOCK, ENTITY, PARTS, UI } from '../../content/copy';
import { CopyButton } from '../copy-button';
import { Body, Clause111, Code, Inline, Verdict } from './primitives';

/* ---------- the parts ---------- */

export function Part({
	id,
	title,
	className,
	children,
}: {
	id: string;
	title: string;
	className?: string;
	children: React.ReactNode;
}) {
	return (
		<section
			className={className ? `part ${className}` : 'part'}
			aria-labelledby={id}>
			<h2 id={id}>{title}</h2>
			{children}
		</section>
	);
}

export function Unreadable() {
	const p = PARTS.unreadable;
	return (
		<Part
			id={p.id}
			title={p.title}
			className='part-unreadable'>
			<div className='part-body'>
				<Body paras={p.body} />
			</div>
			<div className='part-art'>
				<div className='verdict-pair'>
					<div
						className='dialog'
						role='group'
						aria-label={UI.dialogLabel}>
						<p className='dialog-title'>{p.dialog.title}</p>
						<p className='dialog-text'>{p.dialog.text}</p>
						<p className='dialog-buttons'>
							{p.dialog.buttons.map((b) => (
								<span key={b}>{b}</span>
							))}
						</p>
					</div>
					<Verdict
						text={p.verdict}
						passed
					/>
				</div>
			</div>
		</Part>
	);
}

export function Bytes() {
	const p = PARTS.bytes;
	return (
		<Part
			id={p.id}
			title={p.title}
			className='part-bytes'>
			<div className='part-body'>
				<Body paras={p.body} />
				<p className='proof'>
					<Inline text={p.proof} />
				</p>
			</div>
			<div className='part-art'>
				<div className='stack'>
					{p.stack.map((plane) =>
						'changed' in plane ? (
							<div
								key={plane.ref}
								className='plane lifted'>
								<span className='plane-ref'>
									{plane.ref} <b>{plane.clause}</b>
								</span>
								<div className='plane-text'>
									<span
										className='change-bar'
										aria-hidden='true'
									/>
									<Clause111 />
								</div>
							</div>
						) : (
							<div
								key={plane.ref}
								className='plane locked'>
								<span className='plane-ref'>{plane.ref}</span>
								<p
									className={
										'heading' in plane ? 'plane-p plane-head' : 'plane-p'
									}>
									{plane.text}
								</p>
							</div>
						),
					)}
				</div>
				<div className='verdict-pair'>
					<Code
						className='cmp'
						lines={p.run}
					/>
					<Verdict
						text={p.verdict}
						passed
					/>
				</div>
			</div>
		</Part>
	);
}

export function Clauses() {
	const p = PARTS.clauses;
	return (
		<Part
			id={p.id}
			title={p.title}
			className='part-clauses'>
			<div className='part-body'>
				<Body paras={p.body} />
			</div>
			<div className='part-art'>
				<Code
					className='find'
					lines={[{ kind: 'cmd', text: p.command }]}>
					<span className='hits'>
						{p.hits.map((hit) => (
							<span
								className='hit'
								key={hit.ref}>
								<span className='ref'>{hit.ref}</span>{' '}
								<b className='cl'>{hit.clause}</b>{' '}
								<span className='tx'>{hit.text}</span>
							</span>
						))}
					</span>
					<span className='ln out'>{p.total}</span>
				</Code>
			</div>
		</Part>
	);
}

export function Ambiguous() {
	const p = PARTS.ambiguous;
	return (
		<Part
			id={p.id}
			title={p.title}
			className='part-ambiguous'>
			<div className='part-body'>
				<Body paras={p.body} />
			</div>
			<div className='part-art verdict-pair'>
				<Code
					className='ambiguous'
					lines={p.run}>
					<span className='places'>
						{p.places.map((place) => (
							<span
								className='ln out'
								key={place}>
								<Inline
									text={place}
									keep
								/>
							</span>
						))}
					</span>
				</Code>
				<Verdict text={p.verdict} />
			</div>
		</Part>
	);
}

export function Xml() {
	const p = PARTS.xml;
	return (
		<Part
			id={p.id}
			title={p.title}
			className='part-xml'>
			<div className='part-body'>
				<Body paras={p.body} />
			</div>
			<div className='part-art verdict-pair'>
				<pre className='code guard-try'>
					<code>
						<span className='ln cmd'>
							<span className='prompt'>$ </span>
							<s className='struck'>{p.attempt}</s>
						</span>
					</code>
				</pre>
				<Verdict text={p.verdict} />
			</div>
		</Part>
	);
}

/* the For agents prompt, line by line; the phone shows it up to step 3 */
const AGENT_LINES = AGENT_BLOCK.text.split('\n');
const AGENT_SPLIT = AGENT_LINES.findIndex((line) => line.startsWith('3.'));

/* each line of the block hangs; a flag such as -- or --scope never splits
   at its hyphens, only a long URL or the JSON may break inside */
function agentLine(line: string) {
	return (
		<span
			key={line}
			className={/^\s/.test(line) ? 'al al-sub' : 'al'}>
			{line
				.trimStart()
				.split(/(\s+)/)
				.map((w, i) =>
					/^\s+$/.test(w) || w.length > 24 ? (
						w
					) : (
						<span
							key={i}
							className='word'>
							{w}
						</span>
					),
				)}
		</span>
	);
}

/* the last part's artifact is the For agents block: the install as plain
   text an agent can run, with the entity sentence; the same text is in
   index.md and llms.txt */
export function Agents() {
	const p = PARTS.agents;
	return (
		<Part
			id={p.id}
			title={p.title}
			className='part-agents'>
			<div className='part-body'>
				<Body paras={p.body} />
			</div>
			<section
				className='part-art for-agents'
				aria-labelledby={AGENT_BLOCK.id}>
				<div className='for-agents-head'>
					<h3 id={AGENT_BLOCK.id}>{AGENT_BLOCK.title}</h3>
					<CopyButton
						text={AGENT_BLOCK.text}
						label={AGENT_BLOCK.copyLabel}
					/>
				</div>
				<p className='for-agents-lede'>{ENTITY}</p>
				{/* one well; on a phone the first two steps show and the rest wait
				    in a native disclosure, from M up the disclosure is open and its
				    summary hidden (parts.css). Every line stays in the HTML. */}
				<div className='code agents-code'>
					<pre>
						<code>{AGENT_LINES.slice(0, AGENT_SPLIT).map(agentLine)}</code>
					</pre>
					<details className='agents-more'>
						<summary>{AGENT_BLOCK.showAll}</summary>
						<pre>
							<code>{AGENT_LINES.slice(AGENT_SPLIT).map(agentLine)}</code>
						</pre>
					</details>
				</div>
			</section>
		</Part>
	);
}
