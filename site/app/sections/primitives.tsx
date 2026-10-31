import { Fragment } from 'react';
import type * as React from 'react';
import { HERO, type Line, NAV, SITE, UI } from '../../content/copy';
import { CopyButton } from '../copy-button';

/*
 * The page's small parts: inline marks, terminal lines and wells, verdict
 * bars, the install bar and clause 11.1 as Word shows it. Every word comes
 * from content/copy.ts.
 */

/* ---------- inline marks: `code` and **bold**, as in the markdown ---------- */

export function Inline({ text, keep = false }: { text: string; keep?: boolean }) {
	const parts = text.split(/(`[^`]+`|\*\*[^*]+\*\*)/);
	return (
		<>
			{parts.map((part, i) => {
				if (part.startsWith('`') && part.endsWith('`'))
					return <code key={i}>{part.slice(1, -1)}</code>;
				if (part.startsWith('**') && part.endsWith('**'))
					return (
						<b
							key={i}
							className={keep ? 'tok' : undefined}>
							{part.slice(2, -2)}
						</b>
					);
				return keep ? <Tokens key={i} text={part} /> : part;
			})}
		</>
	);
}

/** terminal text that wraps only between tokens: a quoted argument or a
    bracketed match stays on one line, as does a bold run, so "(clause 11.1)"
    never splits */
function Tokens({ text }: { text: string }) {
	return (
		<>
			{text.split(/('[^']*'|"[^"]*"[:,]?|\[[^\]]*\][',]?)/).map((piece, i) =>
				i % 2 ? (
					<span
						key={i}
						className='tok'>
						{piece}
					</span>
				) : (
					piece
				),
			)}
		</>
	);
}

/* ---------- terminal lines, in the well's code style ---------- */

export function Ln({ line, step, keep }: { line: Line; step?: number; keep?: boolean }) {
	return (
		<span
			className={`ln ${line.kind === 'out' ? 'out' : 'cmd'}${line.passed ? ' passed' : ''}`}
			style={
				step === undefined
					? undefined
					: ({ '--step': step } as React.CSSProperties)
			}>
			{line.kind === 'cmd' ? <span className='prompt'>$ </span> : null}
			<Inline
				text={line.text}
				keep={keep}
			/>
		</span>
	);
}

export function Code({
	lines,
	className,
	stagger = false,
	children,
}: {
	lines: readonly Line[];
	className?: string;
	/** the hero's arrival order: output lines rise in one after another */
	stagger?: boolean;
	children?: React.ReactNode;
}) {
	let step = 0;
	return (
		<pre className={className ? `code ${className}` : 'code'}>
			<code>
				{lines.map((line, i) => (
					<Ln
						key={i}
						line={line}
						step={stagger && line.kind === 'out' ? step++ : undefined}
						keep
					/>
				))}
				{children}
			</code>
		</pre>
	);
}

/** stetdoc's verdict on a stet bar; a check that passed lights it court
    green, a refusal stays graphite */
export function Verdict({ text, passed = false }: { text: string; passed?: boolean }) {
	return (
		<p className={passed ? 'verdict verdict-passed' : 'verdict'}>
			<Inline text={text} />
		</p>
	);
}

export function Body({ paras }: { paras: readonly string[] }) {
	return (
		<>
			{paras.map((para) => (
				<p key={para}>
					<Inline text={para} />
				</p>
			))}
		</>
	);
}

/* ---------- the install bar: the mark's bar, carrying the command ---------- */

/** a command that breaks only at its spaces and after a slash, never at a hyphen */
export function Breakable({ text }: { text: string }) {
	return (
		<>
			{text.split(/(\s+)/).map((word, i) =>
				/^\s+$/.test(word) ? (
					word
				) : (
					<span
						key={i}
						className='word'>
						{word.split(/(?<=\/)/).map((piece, j) => (
							<Fragment key={j}>
								{j > 0 ? <wbr /> : null}
								{piece}
							</Fragment>
						))}
					</span>
				),
			)}
		</>
	);
}

export function InstallBar({ command }: { command: string }) {
	return (
		<div className='install'>
			<code className='install-cmd'>
				<span className='prompt'>$ </span>
				<Breakable text={command} />
			</code>
			<CopyButton text={command} />
		</div>
	);
}

export function DocLinks() {
	return (
		<p className='cta-links'>
			<a
				className='text-link'
				href={SITE.links.docs}>
				{NAV.readDocs}
			</a>
			<a
				className='text-link'
				href={SITE.links.github}>
				{NAV.github}
			</a>
		</p>
	);
}


/* ---------- clause 11.1, as Word shows it with the change tracked ---------- */

export function Clause111() {
	const s = HERO.sheet;
	return (
		<p className='doc-p'>
			{s.before}
			<del>
				<span className='sr-only'>{UI.deleted}</span>
				{s.deleted}
			</del>
			<ins>
				<span className='sr-only'>{UI.inserted}</span>
				{s.inserted}
			</ins>
			{s.after}
		</p>
	);
}

