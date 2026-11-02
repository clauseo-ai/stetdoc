/**
 * docx-agent-bench, the figures behind "Fastest on every model."
 * Every number here is real and supplied by Rohan; change none of them
 * without new runs. Times are median seconds per task. `others` is the
 * range across the other setups (no plugin, Anthropic's docx skill,
 * Anthropic's legal plugin, Adeu; the legal plugin was not run on Gemini):
 * fastest to slowest. Source: reports/final-2/tables.md, process metrics,
 * in the docx-agent-bench repository.
 */

import { BENCH_URL, BENCH_GRADING_URL } from '../app/links';

export type BenchRow = {
	model: string;
	/** stetdoc's median time per task, in seconds */
	stetdoc: number;
	/** the other setups: [fastest, slowest], in seconds */
	others: [number, number];
};

export const BENCH_ROWS: BenchRow[] = [
	{ model: 'Claude Haiku 5.5', stetdoc: 21, others: [41, 45] },
	{ model: 'Claude Opus 5.5', stetdoc: 25, others: [37, 56] },
	{ model: 'Gemini 3.8 Flash', stetdoc: 135, others: [274, 538] },
];

export type BenchFact = {
	figure: string;
	line: string;
	/** a wall of this many tiles, every one lit court green: each a run or a
	   markup that came out right. The figure says the same in words. */
	wall?: number;
};

export const BENCH_FACTS: BenchFact[] = [
	{
		figure: '0 of 118',
		line: 'stetdoc runs left Word anything to repair.',
		wall: 118,
	},
	{
		figure: '6 of 6',
		line: 'long Harvey LAB markups passed their checks and opened clean in Word, on every model.',
		wall: 6,
	},
	/* docs/install.md, "Why the channel matters": median of 20 runs of stetdoc --version */
	{ figure: '5 ms', line: 'for each stetdoc call to start.' },
];

/** one piece of the method line: plain text, or a link when `href` is set */
export type MethodPart = { text: string; href?: string };

/** the method line, in order; every figure above links its method from here */
export const BENCH_METHOD: MethodPart[] = [
	{ text: 'docx-agent-bench', href: BENCH_URL },
	{
		text: ': 639 runs in Claude Code 2.1.311 and Antigravity CLI 1.4.8, every file opened in Microsoft Word for Mac 16.114.3. ',
	},
	{ text: 'How it was graded', href: BENCH_GRADING_URL },
	{ text: '.' },
];
