import type { MetadataRoute } from 'next';
import { absolute, sitePages } from '../lib/pages';

export const dynamic = 'force-static';

/** every page with its last real content change; docs come from lib/docs/manifest.ts */
export default function sitemap(): MetadataRoute.Sitemap {
	return sitePages().map((page) => ({
		url: absolute(page.path),
		...(page.updated ? { lastModified: page.updated } : {}),
	}));
}
