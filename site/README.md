# stetdoc.clauseo.chat

The stetdoc launch page: Next.js 16 (App Router, server components only), exported as static files. Every word on the page comes from [content/copy.ts](content/copy.ts), which follows [COPY.md](COPY.md); the benchmark figures live in [content/bench.ts](content/bench.ts).

## Run

```sh
bun install
bun run dev            # http://localhost:3000
bun run build          # next build, then scripts/strip-runtime.ts, into out/
bun run serve          # serves out/ on http://0.0.0.0:3210 with gzip, as Vercel would
bun run typecheck
```

## WebKit screenshots

Every layout check runs in WebKit as well as Chromium, because the page is read in Safari on iPhone, iPad and Mac. On a Mac, `swift site/scripts/webkit-shot.swift --url URL --width 390 --out shot.png` screenshots one page in the system WebKit, with no browser download. Add `--full` for the whole page, `--dark` to match `prefers-color-scheme: dark`, and `--height` for the viewport (default 900). The PNG is one pixel per point, and full pages stop at 16000 px. `--eval 'return …'` runs a script body in the page and prints what it returns, for measuring layout.

The window sits off screen, so WebKit treats the page as hidden and never ticks time-based animations; the script finishes them before the snapshot so the shot shows the resting state (`--no-finish` turns this off). Scroll-driven animations are left to the browser. A full-page shot grows the view to the page's height, so it has no scrollbar, as on iPhone and iPad; a viewport shot on a Mac with a mouse keeps WebKit's 15px classic scrollbar, as Safari would.

`site/scripts/webkit-shots.sh BASE_URL OUTDIR [widths...]` writes full-page day and night shots at each width (default 320 to 1600) as `webkit-{width}-{day|night}.png`. It compiles the Swift helper once to `/tmp/webkit-shot`.

## Deploy on Vercel

The Vercel project `stetdoc` has **Root Directory** set to `site`, and it deploys from the CLI at the repository root, not from `site/`: the build reads `docs/`, the changelog and `Cargo.toml` one level up, so `vercel deploy` run inside `site/` uploads too little and fails. From the root:

```sh
vercel link            # once: the existing stetdoc project
vercel deploy --prod
```

The root `.vercelignore` keeps `target/` and other build output out of the upload. [vercel.json](vercel.json) sets the install command (`bun install`), the build command (`bun run build`), the output directory (`out`) and `"framework": null`, so the site is served as the static files in `out/`. Vercel still applies its Next.js adapter during `next build`, which writes the client reference manifests differently from a local build; `scripts/strip-runtime.ts` runs each manifest rather than parsing its text, so both forms work. An upload has no `.git`, so the docs pages carry no "Updated" dates there. `stetdoc.clauseo.chat` points at the project.

## How it stays fast

- `next.config.ts` sets `output: 'export'` and inlines the CSS.
- `scripts/strip-runtime.ts` runs after `next build` and removes Next's client runtime and RSC payload from out/: the pages have no client components and navigate with full page loads. It fails the build if a page has a client component of ours, or streamed content that needs script, and it checks that no page lost a word. What ships per page: the HTML with its CSS, the Anek font, a 1 KB inline script for the copy buttons, and a 0.5 KB one on the launch page that reads the visitor's system and shows its install (`lib/os-script.ts`). The install picker is CSS only; that script only chooses its first tab.
- Anek Latin is self-hosted in `app/fonts/` under the OFL (`app/fonts/OFL.txt`); the latin face is subset to the weights, widths and characters the page uses. Arial fallbacks with Anek's metrics (globals.css) keep the layout still when the font swaps in.
- `og/` holds a static instance of Anek for the share card, which `app/og.png/route.tsx` draws at build.

`public/samples/contract.docx` is the quickstart's downloadable sample, the only document the repo commits. It is the testkit fixture `contract::package()`, and `cargo test -p stetdoc` fails if its bytes drift (`site_sample_is_the_fixture` in `crates/stetdoc-cli/tests/cli.rs`; `STETDOC_UPDATE_SAMPLE=1` rewrites it). The styles are `app/globals.css` (tokens, base, hero, end) and `app/parts.css` (the parts between them).

Docs links all read `DOCS_URL` in `app/links.ts`, which is `/docs`.
