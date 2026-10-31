import type { NextConfig } from 'next';

// A static site: every route is prerendered into out/, which Vercel serves
// as files. The stylesheet is small enough to inline into the HTML, which
// saves the render-blocking request for it.
const config: NextConfig = {
	output: 'export',
	reactStrictMode: true,
	poweredByHeader: false,
	images: { unoptimized: true },
	experimental: {
		inlineCss: true,
	},
};

export default config;
