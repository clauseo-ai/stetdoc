import { describe, expect, test } from 'bun:test';
import { OS_SCRIPT } from './os-script';

/** runs the script against a stand-in browser and returns what it did */
function run(nav: { platform?: string; userAgent: string; maxTouchPoints?: number; uaPlatform?: string }): {
	os?: string;
	tab?: string;
	moved: number;
} {
	const html: { dataset: Record<string, string> } = { dataset: {} };
	const moved: string[] = [];
	const input = {
		id: '',
		checked: false,
		nextElementSibling: 'label',
		parentNode: { prepend: (...nodes: unknown[]) => moved.push(...nodes.map(String)) },
	};
	let ready = () => {};
	const document = {
		documentElement: html,
		getElementById: (id: string) => ((input.id = id), input),
		addEventListener: (_: string, fn: () => void) => (ready = fn),
	};
	const navigator = {
		platform: nav.platform ?? '',
		userAgent: nav.userAgent,
		maxTouchPoints: nav.maxTouchPoints ?? 0,
		userAgentData: nav.uaPlatform ? { platform: nav.uaPlatform } : undefined,
	};
	new Function('navigator', 'document', OS_SCRIPT)(navigator, document);
	ready();
	return { os: html.dataset.os, tab: input.checked ? input.id : undefined, moved: moved.length };
}

const MAC = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Safari/605.1.15';
const WIN = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36';
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0';
const ANDROID = 'Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36';
const IPHONE = 'Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Mobile/15E148 Safari/604.1';
const CROS = 'Mozilla/5.0 (X11; CrOS x86_64 16000.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36';

describe('os script', () => {
	test('a desktop system gets its own install and picker tab', () => {
		expect(run({ platform: 'MacIntel', userAgent: MAC })).toEqual({ os: 'mac', tab: 'ch-brew', moved: 2 });
		expect(run({ platform: 'Win32', userAgent: WIN, uaPlatform: 'Windows' })).toEqual({ os: 'windows', tab: 'ch-uv', moved: 2 });
		expect(run({ platform: 'Linux x86_64', userAgent: LINUX })).toEqual({ os: 'linux', tab: 'ch-script', moved: 2 });
		expect(run({ userAgent: '', uaPlatform: 'macOS' }).os).toBe('mac');
	});

	test('a phone, a tablet, ChromeOS or an unknown system gets all three', () => {
		const none = { os: undefined, tab: undefined, moved: 0 };
		expect(run({ platform: 'Linux armv8l', userAgent: ANDROID, uaPlatform: 'Android' })).toEqual(none);
		expect(run({ platform: 'iPhone', userAgent: IPHONE })).toEqual(none);
		expect(run({ platform: 'MacIntel', userAgent: MAC, maxTouchPoints: 5 })).toEqual(none);
		expect(run({ platform: 'Linux x86_64', userAgent: CROS, uaPlatform: 'Chrome OS' })).toEqual(none);
		expect(run({ platform: '', userAgent: 'curl/8.7.1' })).toEqual(none);
	});

	test('stays small', () => {
		expect(OS_SCRIPT.length).toBeLessThanOrEqual(500);
	});
});
