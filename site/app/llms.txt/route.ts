import { llmsTxt, textResponse } from '../../lib/markdown';

export const dynamic = 'force-static';

export function GET() {
	return textResponse(llmsTxt());
}
