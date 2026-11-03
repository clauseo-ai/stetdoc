/** Every outbound address on the site. */
export const DOCS_URL = '/docs';
export const GITHUB_URL = 'https://github.com/clauseo-ai/stetdoc';
export const CHANGELOG_URL =
	'https://github.com/clauseo-ai/stetdoc/blob/main/CHANGELOG.md';
export const CLAUSEO_URL = 'https://www.clauseo.chat';
export const SITE_URL = 'https://stetdoc.clauseo.chat';
/** the benchmark behind the figures (COPY.md, Bench) */
export const BENCH_URL = 'https://github.com/clauseo-ai/docx-agent-bench';
/** how the benchmark grades a run, the Word check included (COPY.md, Bench) */
export const BENCH_GRADING_URL = `${BENCH_URL}#how-grading-works`;

/** packaging/install.sh, copied into public/ by scripts/docs-artifacts.ts */
export const INSTALL_SCRIPT_URL = `${SITE_URL}/install.sh`;
/** the suggested install on each desktop system (COPY.md, Hero) */
export const INSTALL_COMMANDS = {
	mac: 'brew install clauseo-ai/tap/stetdoc',
	linux: `curl -fsSL ${INSTALL_SCRIPT_URL} | sh`,
	windows: 'uv tool install stetdoc',
} as const;


/** the agent guide as markdown */
export const AGENT_GUIDE_URL = `${SITE_URL}/docs/agents.md`;
export const RAW_DOCS = 'https://raw.githubusercontent.com/clauseo-ai/stetdoc/main';
export const SECURITY_URL =
	'https://github.com/clauseo-ai/stetdoc/security/advisories/new';
export const SECURITY_POLICY_URL =
	'https://github.com/clauseo-ai/stetdoc/blob/main/SECURITY.md';
