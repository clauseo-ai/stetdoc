/**
 * stetdoc's mark, the proofreader's stet: a graphite bar over three dots,
 * as in the repo's favicon.svg (rect 32 by 8, circles r4 at x 4, 16, 28).
 * Drawn in currentColor, so it is graphite by day and #ECEBE9 by night.
 */
export function StetMark({ className }: { className?: string }) {
	return (
		<svg
			className={className}
			viewBox='0 0 32 22'
			width='32'
			height='22'
			aria-hidden='true'
			focusable='false'>
			<rect
				width='32'
				height='8'
			/>
			<circle
				cx='4'
				cy='18'
				r='4'
			/>
			<circle
				cx='16'
				cy='18'
				r='4'
			/>
			<circle
				cx='28'
				cy='18'
				r='4'
			/>
		</svg>
	);
}

/** The mark's three dots alone, for under the word "stand" in the h1. */
export function StetDots({ className }: { className?: string }) {
	return (
		<svg
			className={className}
			viewBox='0 0 32 8'
			width='32'
			height='8'
			aria-hidden='true'
			focusable='false'>
			<circle
				cx='4'
				cy='4'
				r='4'
			/>
			<circle
				cx='16'
				cy='4'
				r='4'
			/>
			<circle
				cx='28'
				cy='4'
				r='4'
			/>
		</svg>
	);
}
