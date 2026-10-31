---
name: stetdoc
description: The stetdoc launch page. Clauseo's warm concrete and graphite, lit by the court's three colours where they mean something, with the proofreader's stet mark as the page's architecture.
colors:
  # Day. The ground and its planes, low to high (Clauseo's values)
  ground: "#E5E2DC"
  inset: "#DCD8D1"
  well: "#F3F1ED"
  surface: "#FDFCFA"
  sheet: "#FFFFFF"
  # Text tones
  ink: "#23201D"
  ink-hover: "#3A3631"
  ink-press: "#4A453F"
  muted: "#48423B"
  quiet: "#5F584F"
  body: "#3A3631"
  on-ink: "#FDFCFA"
  select: "#E2D9C4"
  # Hairlines
  divide: "rgba(56, 44, 30, 0.1)"
  line: "rgba(56, 44, 30, 0.3)"
  line-strong: "rgba(35, 32, 29, 0.6)"
  # Brick: the Clauseo comma in the footer lockup, nothing else
  brand: "#A3482F"
  # The stet bar: graphite, its words the concrete showing through
  bar: "#23201D"
  on-bar: "#E5E2DC"
  on-bar-quiet: "rgba(229, 226, 220, 0.72)"
  on-bar-line: "rgba(229, 226, 220, 0.45)"
  on-bar-fill: "rgba(229, 226, 220, 0.09)"
  groove: "#DCD8D1"
  # The benchmark on the bar: a groove cut into the bar
  bar-groove: "rgba(0, 0, 0, 0.32)"
  # The depicted Word file
  doc-ink: "#23201D"
  # The court's three colours (Clauseo tokens-colour.css), meaning only
  green: "#2D774A"
  green-text: "#22603A"
  yellow: "#F2B800"
  on-yellow: "#23201D"
  yellow-line: "#B88A00"
  red: "#C2362B"
  red-text: "#9E2B22"
  on-colour: "#FFFFFF"
  flat: "#E9E6E1"
  # The court's word tones on the graphite bar (they invert with the bar)
  on-bar-green: "#A1DFB6"
  on-bar-red: "#F9C1BA"
  on-bar-red-band: "rgba(194, 54, 43, 0.4)"
  # Night: the same concrete unlit, by prefers-color-scheme. Same roles
  night-ground: "#1B1A18"
  night-inset: "#161513"
  night-well: "#31302E"
  night-surface: "#3D3C39"
  night-sheet: "#474644"
  night-ink: "#ECEBE9"
  night-ink-hover: "#D5D4D2"
  night-ink-press: "#C5C4C2"
  night-muted: "#CBCAC8"
  night-quiet: "#B5B4B2"
  night-body: "#D5D4D2"
  night-on-ink: "#1B1A18"
  night-select: "#575552"
  night-divide: "rgba(255, 255, 255, 0.08)"
  night-line: "rgba(236, 235, 233, 0.35)"
  night-line-strong: "rgba(236, 235, 233, 0.7)"
  night-brand: "#D07455"
  night-bar: "#ECEBE9"
  night-on-bar: "#1B1A18"
  night-on-bar-quiet: "rgba(27, 26, 24, 0.74)"
  night-on-bar-line: "rgba(27, 26, 24, 0.45)"
  night-on-bar-fill: "rgba(27, 26, 24, 0.08)"
  night-groove: "#2C2B29"
  night-bar-groove: "rgba(27, 26, 24, 0.1)"
  night-doc-ink: "#ECEBE9"
  night-green: "#317549"
  night-green-text: "#A1DFB6"
  night-yellow: "#E4B336"
  night-on-yellow: "#1B1A18"
  night-yellow-line: "#CFA43A"
  night-red: "#B73E35"
  night-red-text: "#F9C1BA"
  night-flat: "#2F2E2C"
  night-on-bar-green: "#22603A"
  night-on-bar-red: "#9E2B22"
  night-on-bar-red-band: "rgba(183, 62, 53, 0.24)"
typography:
  display:
    fontFamily: "Anek Latin, Anek Display Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "calc((min(100vw, 1440px) - 96px) / 6.4)"
    fontWeight: 620
    lineHeight: 0.84
    letterSpacing: "-0.032em"
    fontVariation: "'wdth' 125"
  headline:
    fontFamily: "Anek Latin, Anek Head Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "clamp(42px, 3.4vw + 22px, 84px)"
    fontWeight: 620
    lineHeight: 1
    letterSpacing: "-0.025em"
    fontVariation: "'wdth' 116"
  title:
    fontFamily: "Anek Latin, Anek Head Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "30px"
    fontWeight: 620
    lineHeight: "36px"
    letterSpacing: "-0.015em"
    fontVariation: "'wdth' 116"
  figure:
    fontFamily: "Anek Latin, Anek Display Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "clamp(64px, 6.4vw + 12px, 128px)"
    fontWeight: 620
    lineHeight: 0.82
    letterSpacing: "-0.035em"
    fontFeature: "'tnum'"
    fontVariation: "'wdth' 125"
  lead:
    fontFamily: "Anek Latin, Anek Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 400
    lineHeight: "33px"
    fontVariation: "'wdth' 104"
  body:
    fontFamily: "Anek Latin, Anek Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "19px"
    fontWeight: 400
    lineHeight: "30px"
    fontVariation: "'wdth' 104"
  label:
    fontFamily: "Anek Latin, Anek Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: "22px"
    fontVariation: "'wdth' 104"
  code:
    fontFamily: "Anek Latin, Anek Narrow Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 400
    lineHeight: "24px"
    fontFeature: "'tnum'"
    fontVariation: "'wdth' 90"
  command:
    fontFamily: "Anek Latin, Anek Narrow Fallback, Segoe UI, system-ui, sans-serif"
    fontSize: "18px"
    fontWeight: 520
    lineHeight: "24px"
    fontFeature: "'tnum'"
    fontVariation: "'wdth' 90"
  document:
    fontFamily: "Charter, Bitstream Charter, Sitka Text, Cambria, Georgia, serif"
    fontSize: "15.5px"
    fontWeight: 400
    lineHeight: "26px"
rounded:
  mark: "2px"
  sheet: "4px"
  bar: "6px"
  dialog: "10px"
  well: "10px"
  dot: "50%"
spacing:
  gutter: "48px"
  gutter-phone: "20px"
  gutter-tablet: "clamp(32px, 9.36px + 3.774vw, 48px)"
  frame-max: "clamp(1440px, 1440px + (100vw - 1440px) * 2 / 3, 1760px)"
  site-gutter: "48px; the M clamp from 600px; 20px under 600px"
  site-edge: "max(site-gutter, 50% - frame-max / 2 + site-gutter)"
  column-gap: "32px"
  hero-gap: "48px"
  part-gap: "200px"
  part-gap-tablet: "152px"
  part-gap-phone: "128px"
  end-gap: "220px"
components:
  install-bar:
    backgroundColor: "{colors.bar}"
    textColor: "{colors.on-bar}"
    typography: "{typography.command}"
    rounded: "{rounded.bar}"
    padding: "6px 6px 6px 0"
  copy-key:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    rounded: "{rounded.sheet}"
    padding: "0 24px"
    height: "48px"
  copy-key-hover:
    backgroundColor: "{colors.surface}"
  install-bar-inverted:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
    typography: "{typography.command}"
    rounded: "{rounded.bar}"
  copy-key-inverted:
    backgroundColor: "{colors.bar}"
    textColor: "{colors.on-bar}"
    rounded: "{rounded.sheet}"
  copy-key-inverted-hover:
    backgroundColor: "{colors.ink-hover}"
  verdict:
    backgroundColor: "{colors.bar}"
    textColor: "{colors.on-bar}"
    typography: "{typography.code}"
    rounded: "{rounded.bar}"
    padding: "16px 22px"
  verdict-passed:
    backgroundColor: "{colors.green}"
    textColor: "{colors.on-colour}"
    typography: "{typography.code}"
    rounded: "{rounded.bar}"
    padding: "16px 22px"
  suggested-chip:
    backgroundColor: "{colors.yellow}"
    textColor: "{colors.on-yellow}"
    rounded: "{rounded.sheet}"
    padding: "0 8px"
    height: "22px"
    backgroundColor: "{colors.yellow}"
    textColor: "{colors.on-yellow}"
    rounded: "{rounded.sheet}"
    padding: "10px 12px 11px"
  wall-tile:
    backgroundColor: "{colors.green}"
    rounded: "{rounded.mark}"
    size: "about 11px, 20 to a row"
  well:
    backgroundColor: "{colors.well}"
    textColor: "{colors.muted}"
    typography: "{typography.code}"
    rounded: "{rounded.well}"
    padding: "28px 32px"
  word-sheet:
    backgroundColor: "{colors.sheet}"
    textColor: "{colors.doc-ink}"
    typography: "{typography.document}"
    rounded: "{rounded.sheet}"
    padding: "40px 32px 48px 56px"
  picker-tab:
    backgroundColor: "{colors.on-bar-fill}"
    textColor: "{colors.on-bar}"
    rounded: "{rounded.sheet}"
    padding: "0 16px"
    height: "44px"
  picker-tab-hover:
    backgroundColor: "{colors.on-bar-line}"
  picker-tab-checked:
    backgroundColor: "{colors.ground}"
    textColor: "{colors.ink}"
  fact-dot:
    backgroundColor: "{colors.bar}"
    rounded: "{rounded.dot}"
    size: "44px"
  bench-bar:
    backgroundColor: "{colors.bar}"
    textColor: "{colors.on-bar}"
    padding: "112px 48px 88px"
  race-track:
    backgroundColor: "{colors.bar-groove}"
    rounded: "{rounded.sheet}"
    height: "48px"
  race-bar:
    backgroundColor: "{colors.green}"
    textColor: "{colors.on-colour}"
    rounded: "{rounded.mark}"
  race-band:
    backgroundColor: "{colors.on-bar-red-band}"
    rounded: "{rounded.mark}"
  dialog:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.ink}"
    rounded: "{rounded.dialog}"
    padding: "22px 24px 20px"
---

# Design System: stetdoc

## Overview

**Creative North Star: "The mark is the page"**

The page is stetdoc's mark, the proofreader's stet, built at the scale of the screen. The bar is a full-bleed graphite plane, and stetdoc's own words are reversed out of it: the claim "Let it stand.", the install command, every verdict, the benchmark, and the closing install section. The three dots are graphite discs on the concrete: they hang once under the word "stand", and they hang under the benchmark bar holding its three facts, so the middle of the page draws the whole mark again. Everything else sits on Clauseo's warm concrete as planes, wells and one white sheet.

The world is inherited. Ground, planes, ink tones, depth recipes, the court's three colours and the brick comma are Clauseo's design system, unchanged. The page is lit the way the Clauseo room is: a tile fills with its court colour when it means something and stays filled, with a neutral shadow under it. Green is every check stetdoc passed and its results; yellow is a tracked change waiting for review and the install that waits for your choice; red, used sparingly, is the other setups in the benchmark. stetdoc adds one material, the stet bar, and a small grammar around it: terminal text set into wells in Anek at 90% width, a depicted Word file as a white turned page, tracked changes drawn in ink, and motion that plays once on load or is driven by the scroll and then stops. Night is the same concrete with the light gone, chosen by `prefers-color-scheme`. At night the mark turns light, so every bar turns light with it and its words turn dark.

Density is low and type is large. The page is long, with wide gaps between parts, and each part pairs a short argument with the real artifact: a terminal run, a Word dialog, a paragraph stack, a benchmark table. Each part is composed for its own content, and no two neighbours share a composition (see Layout).

**Key Characteristics:**
- One material carries stetdoc's voice: the graphite bar, at every scale from a 20px key to the full-bleed h1.
- The stet mark's dots are structure, never decoration: under "stand" once, and on the three facts.
- Anek Latin is the only interface family, and width carries the voice (125%, 116%, 104%, 90%).
- Depth comes from planes, a top-edge highlight and warm-brown shadows. No glass, blur, glow, gradients or outlined boxes.
- Colour means something wherever it appears: green checked, yellow waiting for your hand, red the other side. Nothing else is coloured.
- Strikes are drawn in ink; a tracked change waiting for review lights court yellow, never red or green.
- Motion is CSS only: one load moment, scroll-driven reveals, then stillness.

## Colors

Warm concrete and graphite, lit by the court's three colours at full strength, each only where it means something, and one brick comma.

### Primary
- **Graphite** (`ink`, also `bar`): the stet bar, the mark, the dots, every command line and every emphasis inside a well. The primary text colour on concrete. At night it becomes the near-white `night-ink`, and so does the bar.
- **Concrete through the bar** (`on-bar`): the words on a bar are the ground colour, as if cut out of the plane. Quieter bar text (`on-bar-quiet`) carries prompts, intros and notes; `on-bar-line` underlines links on the bar and fills hovered picker tabs; `on-bar-fill` is a resting picker tab.

### Secondary: the court's three colours
Clauseo's own values, day and night, from tokens-colour.css. Words on green and red are white (`on-colour`); words on yellow are ink (`on-yellow`), never white. Lit things take Clauseo's `--lit` shadow (`--lit-y` on yellow): a top highlight and a short neutral drop.
- **Court green, checked** (`green`, `green-text`): the verdicts that passed (`validated 1 unit: no problems` and `identical`), the bold "no problems" in the hero run, stetdoc's bars in the benchmark and its times (`on-bar-green`, the light tone on the graphite bar), and the facts' tile walls: 118 small tiles for "0 of 118" and six large ones for "6 of 6", every tile lit.
- **Court yellow, waiting for your hand** (`yellow`, `on-yellow`, `yellow-line`): the hero Word sheet's tracked change, a 3px change bar and the margin balloon, the insertion underlined in `yellow-line`; the paragraph that lifts in the bytes stack, its marks in ink; the "Suggested" chip on the hero install and the Homebrew picker tab, and the suggested install's Copy key until it is used.
- **Court red, the other side** (`red`, `on-bar-red`, `on-bar-red-band`): the other setups' range in the benchmark only, a red-tinted band with solid red ends, its figure and its "slowest" label in the light red tone.

### Tertiary
- **Brick** (`brand`): the comma in the Clauseo lockup in the footer. It is the only brick on the page.

### Neutral
- **Warm concrete** (`ground`): the page, the Copy key on a bar, the chosen picker tab, and the inverted install plate inside the closing bar.
- **Concrete inset** (`inset`, also `groove`): the benchmark track, a groove set into the ground.
- **Well** (`well`): terminal text.
- **Surface** (`surface`): the Word "unreadable content" dialog, and the Copy key under the pointer.
- **Sheet** (`sheet`): the depicted Word page and the paragraph planes in the bytes stack.
- **Muted, body, quiet** (`muted`, `body`, `quiet`): the hero sub and well output; running paragraphs; captions, metadata, nav links and refs.
- **Hairlines** (`divide`, `line`, `line-strong`): rule-like separation drawn as 1px inset shadows between benchmark rows and under the sheet's title bar; link underlines at rest and on hover.
- **Flat** (`flat`): an unlit tile, the from-state as a wall tile lights on scroll.

### Named Rules
**The One Brick Rule.** Brick appears only on Clauseo's comma. It is never a fill, a state, an accent or a tracked-change colour.

**The Meaning Only Rule.** Green means a check passed or stetdoc's result; yellow means waiting for the reader's hand (a tracked change to review, the suggested install); red means the other setups. None of them decorates, fills a header, marks a hover or brands a button. An error or a refusal stays graphite: `error[ambiguous_match]` and the guard's block are on graphite, and Word's dialog is concrete.

**The Lit Stays Lit Rule.** A lit tile, verdict or paragraph is lit at rest. Motion may light it as it arrives; nothing dims it again.

**The Neutral Shadow Rule.** A lit thing throws Clauseo's neutral shadow, never one in its own colour. Coloured shadows read as glow.

**The Ink Marks Rule.** Strikes are always ink. A tracked change waiting for review lights yellow: its change bar and balloon in court yellow, its insertion underlined in `yellow-line`. On a yellow plane every mark is `on-yellow`. A tracked change is never red or green.

**The Bar Inverts Rule.** Whatever sits on a bar takes the bar's inverse: concrete words, concrete keys, concrete-tinted fills. Inside the closing bar the install plate inverts too, a concrete plate with a graphite key.

## Typography

**Display Font:** Anek Latin at 125% width (with Anek Display Fallback, metric-matched Arial Bold)
**Body Font:** Anek Latin at 104% width (with Anek Fallback)
**Label/Mono Font:** Anek Latin at 90% width for terminal text and commands (with Anek Narrow Fallback). There is no monospace face.

**Character:** One variable family with four voices. Wide and tight for the claim, the end and the figures; 116% for section heads; 104% for reading; narrow 90% with tabular figures for anything a terminal prints.

The face is self-hosted and subset to weights 400 to 640 and widths 88% to 125%, with a full latin-ext file that is never preloaded. Each width has its own metric fallback so the font swap moves nothing.

### Hierarchy
- **Display** (620, 125% width, line-height 0.84): "Let it stand." on the hero bar, set to fill the frame: one line at frame / 6.4 from 1024px, two lines ("Let it" over "stand.") under it. Where the screen's height caps it, the claim no longer fills the frame and the bar measures its words: it still runs off the left edge, but ends one gutter after "stand." with 6px corners, as a word bar does. Where it fills the frame the bar is full bleed. The closing h2 uses the same voice. Every size is a band token; see Bands below.
- **Figure** (620, 125%, line-height 0.82, tabular): the benchmark's stetdoc times, in `on-bar-green`. The other setups' range sits beside it at 560 weight in `on-bar-red`. The three facts use the same voice.
- **Headline** (620, 116%, line-height 1): each part's h2, scaled to the part's weight from 1024px up (the unreadable head the largest, then agents, bytes, and the narrow ambiguous head). Three parts take the display voice at 125%: the benchmark, the clauses and "Hands off the XML.", set on one line from 1024px. Under 1024px there are two steps only: the argument head and the display head.
- **Title** (620, 116%, 30px / 36px): the "For agents" head, an h3 in the last part.
- **Lead** (400, 104%, 22px / 33px): the hero sub, at most 30em; smaller per band (Bands). The bytes proof line sits at 24px and 112% width.
- **Body** (400, 104%, 19px / 30px, at most 33em): part paragraphs; 18px under 1024px, 17px to 18px under 600px. The page base is 16px / 24px.
- **Label** (400 to 600, 13px to 15px): captions, keys, the sheet's title bar, picker labels, the method note, footer.
- **Code** (400, 90%, 15px / 24px, tabular): well text. Commands step up to 500 in `ink`; bold output is 620. Part wells are 16px / 26px, the clauses well 20px / 32px; under 600px every well, the hero run included, is 15.5px / 25px. The hero well runs from 15px at 1024px to 16px at 1440px with no steps, and its text stops a column short of the sheet that covers its right edge. Where lines must wrap, they break only between tokens: a quoted argument, a bracketed match and a bold run never split.
- **Command** (520, 90%, 18px / 24px): the command on an install bar. 16px on phones.
- **Document** (Charter serif, 15.5px / 26px): text inside the depicted Word file only.

### Named Rules
**The Width Carries Voice Rule.** Hierarchy comes from Anek's width axis as much as its size. Do not add a second family to make a voice.

**The Depicted Document Rule.** The serif belongs to the Word file being depicted, the sheet and the bytes stack. It never sets the page's own words.

**The Narrow Terminal Rule.** Anything a terminal or stetdoc prints is Anek at 90% with tabular figures, including commands quoted in running text, which take 580 weight and no box.

## Layout

The frame is the company site's: 1440px up to a 1440px screen with a 48px gutter, then two thirds of every pixel past it, to 1760px at 1920 and up, with no step; the left edge is computed so content aligns to the frame while bars bleed full width. The gutter and the edge are site tokens on `:root`, `--site-gutter` and `--site-edge` (the left inset of the header's logo at every band), so the launch page's header and footer and the docs' sit on one edge. Parts use a 12-column grid with 32px column gaps, and each part has its own composition, so no two neighbours repeat one:
- **Unreadable:** the heading on columns 1 to 6 beside Word's dialog on 8 to 12; under them the problem paragraph at left and stetdoc's answer at right, under the verdict it gives.
- **Bench:** the stet bar at full bleed. Inside it the display heading takes columns 1 to 7 with the argument at its foot on 8 to 12, then the race across the frame. The three facts hang under the bar on the mark's dots.
- **Bytes:** the paragraph stack leads on columns 1 to 7; the heading, argument and proof sit across from it on 8 to 12.
- **Clauses:** a display heading on columns 1 to 8 with the argument at its foot on 9 to 12, then the search across the whole frame.
- **Ambiguous:** a narrow column of argument on 1 to 4 beside the long refusal on 5 to 12.
- **XML:** one display line across the frame, then the blocked attempt and the guard's bar on 1 to 7 beside the argument on 9 to 12. The attempt is not in a well: it is the command itself, struck large (frame × 0.046) on the bare concrete, and the guard's bar sits square under it, the one verdict that does not rise out of a well.
- **Agents:** the argument on 1 to 5 beside the For agents block on 6 to 12.

Rhythm is generous and stepped: 200px between parts (152px under 1024px, 128px under 600px), 220px before the closing bar (160px under 600px), 120px before the facts.

The hero: the full-bleed bar, the dots centred under "stand" (not the period), then a two-column row with the sub at left and the install box at right (at most 540px), then the scene. The first part starts 152px under the hero (not 200px) from 1024px up, because the hero's foot is the sheet's. In the scene the terminal well spans columns 1 to 8; the Word sheet overlaps its right edge on columns 8 to 12, in front. In L the well and its caption sit level with the middle of the sheet, so no empty field opens under them; from 1440px the well rises to 12px under the install column, 36px above the sheet, so no field opens between the lead and the run.

A text measure set in `em` never shares an element with the frame's edge padding: the edge grows past 1440px, and in a border-box it would eat the measure (the footnote once collapsed to one word per line in Safari on a wide window). The footnote takes the edge as its margins instead, so on a phone it still stops at the right gutter.

### Bands
The page has five width bands and one height band; DESIGN.md, globals.css and parts.css name the same ones. Each band sets its sizes once, as tokens on `.launch` (a wrapper that makes no box); inside a band a size runs from the frame and stops at a share of the screen's height, so nothing jumps between two widths in one band. Heads stop at 12svh (argument) and 15svh (display); the claim at 25svh.

| Band | Widths | Typical screens |
| --- | --- | --- |
| S | under 600px | phones in portrait, 320 to 430 |
| M | 600 to 1023px | iPad portrait, Split View, small windows |
| L | 1024 to 1439px | iPad landscape, 13 to 15 inch laptops |
| XL | 1440 to 1919px | desktops |
| XXL | 1920px and up | large desktops |
| SHORT | landscape and at most 500px tall | phones on their side (568 to 932 wide) |

SHORT comes last and changes only what a short screen needs; everything else keeps the band its width falls in.

| Section | S | M | L | XL | XXL | SHORT |
| --- | --- | --- | --- | --- | --- | --- |
| Frame | 20px gutter | gutter 32px at 600 to 48px at 1024 | 48px gutter, 1440 max | frame grows from 1440px to 1760px at 1920 | 1760px | its width's band |
| Header | 56px | 64px | 72px | 72px | 72px | 56px |
| Claim | two lines, min(frame / 3.6, 14.5svh) | two lines, max(min(frame / 3.6, 12.5svh), min(frame / 6.4, 25svh)); one line on a wide window | one line at frame / 6.4; on a touch screen min(frame / 6.4, 19svh) | one line, min(frame / 6.4, 25svh) | as XL | one line, min(frame / 6.4, 24svh) |
| Lead and install | one column; lead 17px to 19px by height | one column at most 640px, lead and install share a right edge; lead 20px | two columns; lead 20px | two columns; lead 22px | as XL | two columns; lead 17px / 25px, command 16px; under 760px wide lead 16px / 23px, command 15px on one line |
| Scene | stacked, the run a plane edge to edge | stacked, sheet first, well tucked under | side by side, run level with the sheet's middle; run 15px rising to 16px | side by side, the run 12px under the install column and the sheet over its right end; 16px | as XL | stacked as M |
| Part bodies | one column, 17px to 18px | one column, 18px | 12 columns, each body five columns; unreadable paragraphs as one column | 12 columns, the compositions in Layout | as XL | its width's band |
| Part heads | argument clamp(32px, frame × 0.12, 46px), display clamp(40px, frame / 6.6, 72px), so "where 11.1 is." holds one line | argument max(46px, frame × 0.062), display max(72px, frame × 0.11) | per part from the frame as XL, but clauses sets "It knows where" on one line in its seven columns (their width / 6.6) | per part from the frame (unreadable frame × 0.0588, bench × 0.0677, bytes × 0.0449, clauses × 0.098, ambiguous × 0.0364, xml frame / 12.6, agents × 0.0474) | as XL | capped by height (12svh, 15svh) |
| Benchmark rows | model over the two times, track under; each range on one line, its size stopping where "274 to 538 s" fills its column | model and the two times on one line, track under | one line: model, time, track, range, the track level with the figures' middle | as L | as L | its width's band |
| Facts | each fact a row: dot beside its figure, both walls and the line in the text column beside the dot | each fact a row: dot, figure and line, wall at right; "1 ms", which has no wall, sets its line in the wall's column; the stack follows 112px under | three up, every line level on one row | as L | as L | three up, figure min((frame - 64px) / 3 / 4.5, 14svh) |
| For agents | wraps, hanging indents; the first two steps, the rest behind a native disclosure, "Show all five steps" | the whole prompt | as M | as M | as M | its width's band |
| End | one column, head at the display step | one column, "Install stetdoc." on one line at frame / 8.4 | head and picker five to seven | head beside a 600px picker | as XL | its width's band |
| Picker keys | three over three | one row; three over three in a picker under 600px wide | three over three to about 1170px, then one row | one row | one row | its width's band |
| Footer | lockup, then links three over two | lockup, then links in a row | one row | one row | one row | its width's band |

- Under 1024px the hero row and the scene stack, with the sheet first and the well tucked 32px under it; under 600px the run is a plane edge to edge, square, and the sheet lies on it 10px in from each side.
- Under 600px the picker becomes a three-column grid of keys, and the For agents Copy key sits in the block's head row, beside its heading, so it never covers text. The key copies the whole prompt even while the disclosure is shut, and every line of the prompt stays in the HTML. From 600px the disclosure is open in effect (`::details-content`) and its summary hidden.
- Any picker narrower than its six keys' 594px sets them three over three (a container query), never five over one.
- WebKit's `text-wrap: pretty` shortens every line of a paragraph in a narrow column, so under 600px paragraphs wrap plainly (this rule is site-wide, docs included).

### One-column composition, part by part
Three critiques found every phone part falling into heading, text, well, verdict. Under 1024px (S and M, wherever the parts are one column) each part takes its own order, decided here, and no two neighbours share one:
- **Unreadable:** the artifact leads. Word's dialog and the green verdict rising out of it come first, then the heading (which echoes the dialog's words), the problem and the answer.
- **Bench:** the one full-bleed graphite plane mid-page. The times are giant (to 64px) in light green, the other setups' range in light red beside them, and every track ends at its row's slowest setup, labelled there in red.
- **Facts:** each fact hangs from its own dot. Under 600px the dot sits beside its figure, the 118 green tiles run across the text column under "0 of 118", the six large tiles stand in a block under "6 of 6" in the same column, and each line sits under its figure. From 600px the dot, the figure with its line, and the wall stand in one row.
- **Bytes:** the artifact leads. The paragraph stack starts at the lifted yellow paragraph (the two locked planes above it drop out, so it never arrives as a stray fragment after the facts) and runs edge to edge: the yellow paragraph off both sides of the screen with square corners, the locked paragraphs after it inset on the concrete. Then the round trip and its green "identical", then the heading and the argument.
- **Clauses:** the display heading, then the search edge to edge with its clause numbers at 42px, then the argument under it as its caption, in 16px quiet type.
- **Ambiguous:** the artifact leads again. The refusal and its graphite bar come first, then the heading, then the argument.
- **XML:** the display heading, the blocked attempt struck large on the bare concrete with the guard's bar square under it, then the argument.
- **Agents:** the argument, then the For agents block with its own head.

### Named Rules
**The Phone Mark Rule.** On a phone each fact hangs from its own dot, set beside its figure, so a dot always holds the fact it belongs to. A row of three dots spread to the gutters read as decoration, unrelated to the stack below.

**The No Formula Rule.** No two neighbouring parts share an order on a phone. At least one part leads with its artifact, one runs its artifact to the screen edges, one sits on a full-bleed plane, and one sets its words beside the artifact as a caption.

**The Well Wraps, Not the Page Rule.** On phones long terminal lines never push the page sideways and never hide past the well's edge. They wrap inside the well with a hanging indent, breaking only between tokens; the For agents text keeps its own indents and may break a long URL anywhere.

## Elevation & Depth

Depth is Clauseo's: planes at different heights, a 1px top-edge highlight, and warm-brown shadows (rgba(56, 44, 30, x)) by day, black shadows at night. Wells and grooves are set into the wall with an inner shade at the top and a light lip at the bottom. Nothing uses blur, glass or glow.

### Shadow Vocabulary
- **Bar** (`--shadow-bar`): every stet bar (h1, install, verdicts, closing bar) and the graphite key inside the closing bar. A faint top highlight and a long, low warm shadow.
- **Key** (`--shadow-key`): the concrete Copy key on a bar and the chosen picker tab. A bright top edge with a short dark drop.
- **Page** (`--shadow-page`): the Word sheet, a turned page lying in front.
- **Raised** (`--e2`): the Word dialog and the lifted paragraph in the bytes stack.
- **Low** (`--e1`): the dialog's buttons.
- **Flat** (`--shadow-flat`): the locked paragraph planes, barely off the ground.
- **Set in** (`--set-in`): wells and the inverted install plate in the closing bar. A cut into the wall: a 2px inner shade at the top, a 1px dark lip under the top edge, a faint ring and a light lip at the bottom. The well tone sits in it, so verdict bars rise out of the wells.
- **Groove in the bar** (`--bar-groove-shade`): the benchmark tracks, cut into the graphite plane.

### Named Rules
**The Planes Not Lines Rule.** Separation comes from a plane's height or a 1px inset hairline, never from an outlined box or a hard rule.

**The Lift Means Changed Rule.** In the bytes stack the changed paragraph alone is raised; locked paragraphs stay flat. Height marks what stetdoc touched.

## Shapes

Corners are small and purposeful. Track marks and the benchmark bar are 2px; the Word sheet, paragraph planes, keys, picker tabs and the track are 4px; bars that carry words (install, verdicts) are 6px; the Word dialog and the wells are 10px. The full-bleed bars (h1, closing section) are square because they run off both edges. The dots are true circles. The mark itself is a 32 by 8 bar over three r4 circles, drawn in `currentColor`.

## Components

### Install bar
A small stet bar carrying the command, and its key.
- **Shape:** 6px corners, 6px inner padding on top, right and bottom.
- **Content:** a quiet `$` prompt that cannot be selected, then the command in the command voice. Commands break only at spaces and after a slash, never at a hyphen.
- **Copy key:** a concrete key (48px tall, 4px corners, key shadow) that turns surface under the pointer and drops 1px when pressed. "Copy" and "Copied" share one grid cell so the key never changes width.
- **Suggested:** the label over the hero install is the court-yellow "Suggested" chip, 22px tall. The suggested install's Copy key (the hero's, the Homebrew panel's in the closing bar, and the For agents key, the same install for an agent) is court yellow with ink words and `--lit-y` (`chip-yellow` under the pointer) until it is used once; then the script marks it `data-chosen`, the yellow goes and the key is plain again, as Clauseo drops yellow once a choice is made.
- **Inverted:** inside the closing bar the bar becomes a set-in concrete plate and the key becomes graphite with the bar shadow, `ink-hover` under the pointer.
- **Focus:** a 2px outline in the colour of the bar's text, 2px out.

### Verdict bar
stetdoc's answer, rising out of what it answers. One device in two lights.
- **Style:** Anek 90% at 16px / 25px, 16px 22px padding, 6px corners.
- **Passed:** a check that passed (`validated 1 unit: no problems`, `identical`) is lit court green with white words and the `--lit` shadow.
- **Refused:** a refusal (`next: Nothing was applied`, the guard's block) stays graphite with concrete words and the bar shadow. Refusals are never red.
- **Placement:** it overlaps the foot of what it answers by 18px and steps in 40px from its left edge (16px on phones), running to the right edge. The same under a well and under the Word dialog; the well or dialog keeps room at its foot so nothing hides under the bar.

### Well
Terminal text set into the wall.
- **Style:** `well` fill, set-in shadow, 10px corners, 28px 32px padding in the parts, code voice, `muted` output, `ink` commands at 500, a hanging indent of 1.1em.
- **Spacing:** 4px between lines, 12px after a command (18px in the hero well).
- **Phones:** lines wrap inside the well, between tokens, with the 1.1em hanging indent; nothing scrolls sideways or clips.

### Word sheet
A white turned page, the file stetdoc wrote.
- **Style:** `sheet` fill, page shadow, 4px corners, a centred 13px file-name bar with a 1px divide under it, then the page in Charter.
- **Tracked change, waiting for review:** deletion in `quiet` with a 1.5px ink strike; insertion underlined with a 2px `yellow-line` inset; a 3px court-yellow change bar in the left margin; a margin balloon lit court yellow (`--lit-y`) with author, change and time in `on-yellow`, joined by a dotted `yellow-line` lead. The paragraph after is quiet and cut at two lines.

### Picker
Radio inputs and `:checked`, no script. Every command is in the HTML.
- **Tabs:** keys on the bar, 44px tall, 4px corners, `on-bar-fill` at rest, `on-bar-hover` (24% concrete) under the pointer. The chosen tab becomes a concrete key with the key shadow. The suggested channel carries the "Suggested" chip: court yellow, `on-yellow` words, 4px corners, `--lit-y`, the same chip as over the hero install.
- **Panel:** with `:has`, only the chosen channel's install bar and note show.
- **Phones:** a three-column grid of 52px keys, name over tag.

### Benchmark
A real table drawn on the stet bar itself, at true scale, read at a glance.
- **Bar:** the part is a full-bleed graphite plane with the bar shadow, 112px 48px 88px padding (64px 20px 56px on phones). Its heading, argument, caption and method are reversed out of it.
- **Row:** the model name, then stetdoc's time and the other setups' range head to head, each under a small label ("stetdoc", "Other setups"): stetdoc's in the figure voice in `on-bar-green`, the range smaller in `on-bar-red`, its box as tall as stetdoc's figure so the two labels sit on one line. All rows share one grid, so each column sits at one x down the table. From 1024px a row runs on one line, the track between the two times and level with their middle; under 1024px the track runs full width under them. The track is a 40px groove (44px under 1024px) cut into the bar, running from 0 to the row's slowest setup and labelled at both ends ("0", and "slowest 45 s" in `on-bar-red` under the red band's end), so each row's scale explains itself. The stetdoc bar is court green at its true length with the `--lit` shadow, labelled in white; the other setups are a red-tinted band with 3px solid red ends. The key repeats both swatches. A 1px divide separates rows.
- **Facts:** three facts hang under the bar, each under a 44px graphite dot, figures in display voice, on one subgrid so their lines sit level. "0 of 118" and "6 of 6" each carry a tile wall (`wall`): 118 tiles 20 to a row and a three by two block of 40px tiles, every tile court green with `--lit`, the figure carrying the count in words and the wall hidden from assistive tech. "1 ms" has no wall, so its line sits right under its figure and the space falls at the column's foot. On scroll each tile lights from `flat` as it comes on screen and stays lit.

### For agents
The install as plain text an agent can run, as the last part's artifact. A head row with the "For agents" title and a graphite Copy key, the entity sentence under it, then a well with the text, set `white-space: pre-wrap`: it wraps at every width with hanging indents, never scrolls, and breaks a long URL or the JSON anywhere. Its anchor is `#agents-install`.

### Navigation
The mark and the lowercase wordmark (Anek 125%, 600, 25px) at left; "Docs" as a word and GitHub's mark (the official Octicon mark-github, inline SVG at 20px in `currentColor`, named "stetdoc on GitHub") at right in `quiet` at 450, `ink` under the pointer, 44px targets. The footer repeats them, with the changelog, beside "from" and the Clauseo lockup. Docs links read `DOCS_URL` in app/links.ts.

### Text links
Underlined at 1px, 5px clear of the descenders, in `line` at rest and `line-strong` under the pointer. Vertical padding with matching negative margin gives a 44px target without moving the line.

### Motion
One load moment and one scroll grammar, all CSS, ease `cubic-bezier(0.16, 1, 0.3, 1)`. Resting styles are the end state, and reduced motion removes every animation and transition.
- **Load:** the three dots under "stand" scale in one after another (200ms each, starting at 200ms, 80ms apart). Then the hero run: well output lines rise 5px one by one (130ms apart), the strike draws over the old words at 940ms, the insertion and change bar come in at 1180ms, and the balloon slides in at 1340ms.
- **Scroll** (`animation-timeline: view()`): the benchmark bars draw to length and the ranges fade in; the fact dots seat in turn; each wall tile lights from `flat` to green as it comes on screen; the changed paragraph lifts from flat; strikes draw; verdict bars slide in 24px from the left. Every range ends by the time the element is wholly on screen (`entry 100%` at the latest), and the fill is backwards only, so an element in view is always at rest, and with no timeline support the resting style shows.
- **Under 1024px:** the hero's change plays by scroll instead of on load, because the stacked sheet sits at the foot of the first screen, on the same rule.
- **State:** hover and colour changes take 140ms.

## Do's and Don'ts

### Do:
- **Do** put stetdoc's own words on a graphite bar (`bar` with `on-bar` text and the bar shadow): claims, commands, refusals, results; a verdict that passed lights green instead.
- **Do** use the mark's dots only where the mark would put them: hanging under a bar, or holding exactly three items.
- **Do** set terminal output and commands in Anek at 90% width with tabular figures, inside a set-in well.
- **Do** light a thing in its court colour only when it means that: green checked, yellow waiting for your hand, red the other side, with a neutral `--lit` shadow, and keep it lit.
- **Do** draw strikes in ink, and light a tracked change waiting for review in court yellow: the change bar, the balloon, the `yellow-line` underline.
- **Do** keep every motion either a single load moment or driven by the scroll, with the resting style as the end state.
- **Do** give every key, tab and link a 44px target.

### Don't:
- **Don't** use brick anywhere except Clauseo's comma.
- **Don't** colour tracked changes red or green, or an error or refusal red.
- **Don't** put white words on yellow, or give a lit tile a shadow in its own colour.
- **Don't** add a monospace or a second sans; Anek's width axis is the range.
- **Don't** use the serif outside the depicted Word file.
- **Don't** separate with outlined boxes or hard rules; use planes and 1px inset hairlines.
- **Don't** use glass, blur, glow or gradients.
- **Don't** make the page scroll sideways on a phone; long terminal lines wrap inside their well.
