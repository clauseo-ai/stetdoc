import type { MetadataRoute } from 'next';
import { SITE_URL } from './links';

export const dynamic = 'force-static';

/*
 * Every crawler is welcome. The named group only states that intent for the
 * search, user-fetch and training bots by name: the * group already admits
 * them. A named group replaces * for that bot (RFC 9309 2.2.1), so if a
 * Disallow is ever added to *, copy it into the named group too.
 */
export default function robots(): MetadataRoute.Robots {
	return {
		rules: [
			{ userAgent: '*', allow: '/' },
			{
				userAgent: [
					'OAI-SearchBot',
					'ChatGPT-User',
					'GPTBot',
					'Claude-SearchBot',
					'Claude-User',
					'ClaudeBot',
					'PerplexityBot',
					'Perplexity-User',
					'Googlebot',
					'Google-Extended',
					'Google-CloudVertexBot',
					'bingbot',
					'Applebot',
					'Applebot-Extended',
					'meta-externalagent',
					'meta-externalfetcher',
					'meta-webindexer',
					'Amazonbot',
					'Amzn-SearchBot',
					'Amzn-User',
					'MistralAI-User',
					'MistralAI-Index',
					'DuckAssistBot',
					'DuckDuckBot',
					'YouBot',
					'CCBot',
				],
				allow: '/',
			},
		],
		sitemap: `${SITE_URL}/sitemap.xml`,
	};
}
