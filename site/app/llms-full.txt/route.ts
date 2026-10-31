import { llmsFullTxt, textResponse } from '../../lib/markdown';

/** every page's markdown in one file: the launch page, then each docs page */
export const dynamic = 'force-static';

export function GET() {
	return textResponse(llmsFullTxt());
}
