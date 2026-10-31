import { SECURITY_POLICY_URL, SECURITY_URL, SITE_URL } from '../../links';

/** RFC 9116. Expires is set eleven months past each build, so every deploy renews it. */
export const dynamic = 'force-static';

export function GET() {
	const expires = new Date();
	expires.setUTCMonth(expires.getUTCMonth() + 11);
	expires.setUTCHours(0, 0, 0, 0);
	const body = [
		`Contact: ${SECURITY_URL}`,
		'Contact: mailto:rohan@clauseo.chat',
		`Expires: ${expires.toISOString()}`,
		'Preferred-Languages: en',
		`Policy: ${SECURITY_POLICY_URL}`,
		`Canonical: ${SITE_URL}/.well-known/security.txt`,
		'',
	].join('\n');
	return new Response(body, {
		headers: { 'Content-Type': 'text/plain; charset=utf-8' },
	});
}
