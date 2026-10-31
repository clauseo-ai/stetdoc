import { homeMarkdown, markdownResponse } from '../../lib/markdown';

/** the launch page as markdown, from the same content as the HTML */
export const dynamic = 'force-static';

export function GET() {
	return markdownResponse(homeMarkdown());
}
