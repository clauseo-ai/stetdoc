/**
 * docx-agent-bench, the figures behind "Done first, on every model."
 * Every number here is real and supplied by Rohan; change none of them
 * without new runs. Times are median seconds per task. `others` is the
 * range across the four other setups (no plugin, Anthropic's docx skill,
 * Anthropic's legal plugin, Adeu): fastest to slowest.
 */

export type BenchRow = {
	model: string;
	/** stetdoc's median time per task, in seconds */
	stetdoc: number;
	/** the other four setups: [fastest, slowest], in seconds */
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
		line: 'stetdoc runs that wrote a file Word offered to repair.',
		wall: 118,
	},
	{
		figure: '6 of 6',
		line: 'long Harvey LAB markups, fully correct on every model.',
		wall: 6,
	},
	{ figure: '1 ms', line: 'from launch to a ready MCP server.' },
];

export const BENCH_METHOD =
	'docx-agent-bench, 639 runs in Claude Code 2.1.311 and Antigravity CLI 1.4.8. Five runs per task, medians per run, and every file opened in Microsoft Word for Mac 16.114.3.';
