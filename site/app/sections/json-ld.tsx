import { SITE, UI } from '../../content/copy';
import { stetdocVersion } from '../../lib/version';
import { SITE_URL } from '../links';

/* entity data for search and answer engines; every claim is on the page */
export function JsonLd() {
	const org = 'https://www.clauseo.chat/#org';
	const ld = {
		'@context': 'https://schema.org',
		'@graph': [
			{
				'@type': 'WebSite',
				'@id': `${SITE.url}/#website`,
				name: SITE.name,
				url: `${SITE.url}/`,
				description: SITE.description,
				inLanguage: 'en',
				publisher: { '@id': org },
			},
			{
				'@type': 'Organization',
				'@id': org,
				name: 'Clauseo',
				url: 'https://www.clauseo.chat/',
				logo: 'https://www.clauseo.chat/apple-icon.png',
				sameAs: ['https://github.com/clauseo-ai'],
			},
			{
				'@type': 'SoftwareApplication',
				'@id': `${SITE.url}/#app`,
				name: SITE.name,
				description: UI.jsonLdDescription,
				url: `${SITE.url}/`,
				applicationCategory: 'DeveloperApplication',
				operatingSystem: 'macOS, Linux, Windows',
				softwareVersion: stetdocVersion(),
				offers: { '@type': 'Offer', price: 0, priceCurrency: 'USD' },
				isAccessibleForFree: true,
				license: 'https://github.com/clauseo-ai/stetdoc/blob/main/LICENSE-MIT',
				downloadUrl: 'https://github.com/clauseo-ai/stetdoc/releases',
				installUrl: `${SITE.url}/#install`,
				softwareHelp: { '@type': 'CreativeWork', url: `${SITE_URL}${SITE.links.docs}` },
				publisher: { '@id': org },
			},
		],
	};
	return (
		<script
			type='application/ld+json'
			dangerouslySetInnerHTML={{
				__html: JSON.stringify(ld).replace(/</g, '\\u003c'),
			}}
		/>
	);
}

