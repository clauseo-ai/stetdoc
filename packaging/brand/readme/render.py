"""Write the art in the root README: the banner and the benchmark chart, each by day and by night, wide and narrow.

Run from the repository root:
    uv run --no-project --with fonttools --with brotli --with uharfbuzz python packaging/brand/readme/render.py

The art follows the launch page's design in site/DESIGN.md. The stet bar is graphite with its words in concrete, the
mark's three dots hang under "stand", and the court's colours appear only where they mean something. Green is stetdoc's
result in the benchmark and red is the other setups. Brick is Clauseo's alone, so it never appears here.

GitHub shows an SVG through <img>, which loads no web font, so this script draws every word as outlines from the Anek
Latin the site hosts in site/app/fonts/, under the OFL. HarfBuzz shapes each line at the site's weight and width. The README
picks a file with <picture>, night when the reader's system is dark and narrow on a phone. A renderer without
<picture> shows the day wide file.

The figures are site/content/bench.ts, the same ones the launch page draws.
"""

from __future__ import annotations

import io
from pathlib import Path

import uharfbuzz as hb
from fontTools.ttLib import TTFont

HERE = Path(__file__).parent
ROOT = HERE.parent.parent.parent
FONT = ROOT / "site/app/fonts/AnekLatin-latin-subset.woff2"

# site/DESIGN.md, colors. Day, then night: the same roles with the light gone, and the bar turned light.
DAY = {
    "ground": "#E5E2DC",
    "ink": "#23201D",
    "bar": "#23201D",
    "on_bar": "#E5E2DC",
    "on_bar_quiet": "#E5E2DC",
    "on_bar_quiet_a": 0.72,
    "divide": "#E5E2DC",
    "divide_a": 0.12,
    "groove": "#000000",
    "groove_a": 0.32,
    "green": "#2D774A",
    "on_bar_green": "#A1DFB6",
    "red": "#C2362B",
    "red_band_a": 0.4,
    "on_bar_red": "#F9C1BA",
    "on_colour": "#FFFFFF",
}
NIGHT = {
    "ground": "#1B1A18",
    "ink": "#ECEBE9",
    "bar": "#ECEBE9",
    "on_bar": "#1B1A18",
    "on_bar_quiet": "#1B1A18",
    "on_bar_quiet_a": 0.74,
    "divide": "#1B1A18",
    "divide_a": 0.12,
    "groove": "#1B1A18",
    "groove_a": 0.1,
    "green": "#317549",
    "on_bar_green": "#22603A",
    "red": "#B73E35",
    "red_band_a": 0.24,
    "on_bar_red": "#9E2B22",
    "on_colour": "#FFFFFF",
}

# site/content/bench.ts: median seconds per task, stetdoc and the other four setups' fastest and slowest
BENCH = [
    ("Claude Haiku 5.5", 21, (41, 45)),
    ("Claude Opus 5.5", 25, (37, 56)),
    ("Gemini 3.8 Flash", 135, (274, 538)),
]

# the site's voices: (weight, width, tracking in em, OpenType features)
DISPLAY = (620, 125, -0.032, {})
WORDMARK = (600, 125, -0.015, {})
FIGURE = (620, 125, -0.035, {"tnum": True})
RANGE = (560, 125, -0.02, {"tnum": True})
TITLE = (620, 116, -0.015, {})
BODY = (400, 104, 0.0, {})
LABEL = (400, 104, 0.0, {"tnum": True})


# Each file is drawn on a grid wider than GitHub shows it: the wide files at about 850 px, the narrow ones at about
# 324 px on a 390 px phone. U is how many units make one shown pixel, so the corners keep DESIGN.md's scale.
U_WIDE = 1600 / 850
U_NARROW = 700 / 324


def num(n: float) -> str:
    return f"{round(n, 1):g}"


class Pen:
    """Collects a glyph's outline as SVG path commands in font units, with y up as the font draws it."""

    def __init__(self):
        self.d: list[str] = []
        self.xs: list[float] = []

    def p(self, pt) -> str:
        self.xs.append(pt[0])
        return f"{round(pt[0])} {round(pt[1])}"

    def moveTo(self, pt):
        self.d.append("M" + self.p(pt))

    def lineTo(self, pt):
        self.d.append("L" + self.p(pt))

    def qCurveTo(self, *pts):
        self.d.append("Q" + " ".join(self.p(q) for q in pts))

    def curveTo(self, *pts):
        self.d.append("C" + " ".join(self.p(q) for q in pts))

    def closePath(self):
        self.d.append("Z")


class Anek:
    """Anek Latin, shaped by HarfBuzz at any weight and width on its axes. Each glyph is drawn once per file, in
    <defs>, and every letter is a <use> of it, which keeps the files small."""

    def __init__(self):
        tt = TTFont(FONT)
        tt.flavor = None
        data = io.BytesIO()
        tt.save(data)
        self.face = hb.Face(data.getvalue())
        self.upem = self.face.upem
        self.cap = tt["OS/2"].sCapHeight / self.upem
        self.glyphs: dict[tuple, tuple[str, str, float, float]] = {}
        self.used: dict[str, str] = {}

    def begin(self) -> None:
        """Starts a new file: no glyph is defined in it yet."""
        self.used = {}

    def defs(self) -> str:
        return "".join(f'<path id="{gid}" d="{d}"/>' for gid, d in self.used.items())

    def _shape(self, text: str, voice: tuple):
        wght, wdth, track, features = voice
        font = hb.Font(self.face)
        font.set_variations({"wght": wght, "wdth": wdth})
        buf = hb.Buffer()
        buf.add_str(text)
        buf.guess_segment_properties()
        hb.shape(font, buf, features)
        return font, buf.glyph_infos, buf.glyph_positions, track * self.upem

    def _glyph(self, font, voice: tuple, g: int) -> tuple[str, str, float, float]:
        key = (voice[0], voice[1], g)
        if key not in self.glyphs:
            pen = Pen()
            font.draw_glyph_with_pen(g, pen)
            name = f"g{voice[0]}{voice[1]}-{g}"
            self.glyphs[key] = (name, "".join(pen.d), min(pen.xs, default=0), max(pen.xs, default=0))
        return self.glyphs[key]

    def width(self, text: str, size: float, voice: tuple) -> float:
        """The advance of the line, without the tracking after its last letter, as CSS would lay it out."""
        _, _, pos, track = self._shape(text, voice)
        units = sum(p.x_advance + track for p in pos) - track
        return units * size / self.upem

    def run(self, text: str, voice: tuple) -> tuple[list[tuple[str, float, float]], float, float]:
        """The line in font units: each letter's glyph and pen position, and the ink's left and right edges."""
        font, infos, pos, track = self._shape(text, voice)
        out = []
        pen_x = 0.0
        left, right = float("inf"), float("-inf")
        for info, p in zip(infos, pos):
            name, d, x0, x1 = self._glyph(font, voice, info.codepoint)
            x = pen_x + p.x_offset
            if d:
                out.append((name, x, p.y_offset))
                self.used.setdefault(name, d)
                left, right = min(left, x + x0), max(right, x + x1)
            pen_x += p.x_advance + track
        return out, left, right

    def ink(self, text: str, size: float, voice: tuple) -> tuple[float, float]:
        """The ink's left and right edges of the line set at 0, at this size."""
        used = dict(self.used)
        _, left, right = self.run(text, voice)
        self.used = used
        k = size / self.upem
        return left * k, right * k


A = Anek()


def text(s: str, x: float, y: float, size: float, voice: tuple, fill: str, anchor: str = "start", alpha: float = 1.0) -> str:
    """The line with its baseline at y, as uses of its glyphs, scaled from font units and flipped."""
    if anchor != "start":
        w = A.width(s, size, voice)
        x -= w if anchor == "end" else w / 2
    letters, _, _ = A.run(s, voice)
    k = size / A.upem
    op = f' fill-opacity="{alpha:g}"' if alpha < 1 else ""
    uses = "".join(f'<use href="#{n}" x="{round(gx)}"' + (f' y="{round(gy)}"' if gy else "") + "/>" for n, gx, gy in letters)
    return f'<g fill="{fill}"{op} transform="matrix({k:.5g} 0 0 {-k:.5g} {num(x)} {num(y)})">{uses}</g>'


def svg(w: float, h: float, title: str, body: str, defs: str = "") -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{num(w)}" height="{num(h)}" viewBox="0 0 {num(w)} {num(h)}"'
        f' role="img"><title>{title}</title><defs>{A.defs()}{defs}</defs>{body}</svg>\n'
    )


def mark(x: float, y: float, width: float, fill: str) -> str:
    """The stet: a 32 by 8 bar over three r4 dots, on the 32-unit grid of packaging/brand/render.py."""
    k = width / 32
    parts = [f'<rect fill="{fill}" x="{num(x)}" y="{num(y)}" width="{num(32 * k)}" height="{num(8 * k)}"/>']
    parts += [f'<circle fill="{fill}" cx="{num(x + cx * k)}" cy="{num(y + 18 * k)}" r="{num(4 * k)}"/>' for cx in (4, 16, 28)]
    return "".join(parts)


def dots(cx: float, top: float, width: float, fill: str) -> str:
    """The mark's three dots alone, centred on cx, as they hang under "stand"."""
    k = width / 32
    x = cx - width / 2
    return "".join(f'<circle fill="{fill}" cx="{num(x + c * k)}" cy="{num(top + 4 * k)}" r="{num(4 * k)}"/>' for c in (4, 16, 28))


# ---------- the banner: the site's first screen, the mark at the scale of the plate ----------


def banner(c: dict, narrow: bool) -> str:
    """The lockup on the concrete, then the bar running edge to edge with "Let it stand." reversed out of it, and the
    three dots hanging under "stand". Wide, the claim fills the frame on one line (frame / 6.32, the claim's width in
    its own em); narrow, it takes two lines, "Let it" over "stand.", as the site sets it under 1024px."""
    W = 700 if narrow else 1600
    u = U_NARROW if narrow else U_WIDE
    G = 40 if narrow else 72  # the gutter
    body = []

    # the lockup, as the site's header sets it: the mark 1.04 em wide, 0.44 em before the word, on a 1.28 em line
    wm = 44 if narrow else 54
    line_top = 40 if narrow else 60
    line_h = 1.28 * wm
    base = line_top + line_h / 2 + A.cap * wm / 2
    mw = 1.04 * wm
    mh = mw * 22 / 32
    body.append(mark(G, line_top + (line_h - mh) / 2 + wm / 25, mw, c["ink"]))
    body.append(text("stetdoc", G + mw + 0.44 * wm, base, wm, WORDMARK, c["ink"]))

    bar_top = line_top + line_h + (36 if narrow else 52)
    lines = ["Let it", "stand."] if narrow else ["Let it stand."]
    widest = max(A.width(s, 1, DISPLAY) for s in lines)
    S = (W - 2 * G) / widest
    lead = (0.86 if narrow else 0.84) * S
    pad_top = (0.08 if narrow else 0.075) * S
    # the cap top and the last baseline sit an equal step in from the bar's edges
    first_base = bar_top + 0.1 * S + A.cap * S
    last_base = first_base + lead * (len(lines) - 1)
    bar_bottom = last_base + 0.1 * S
    body.append(f'<rect fill="{c["bar"]}" x="0" y="{num(bar_top)}" width="{W}" height="{num(bar_bottom - bar_top)}"/>')
    for i, s in enumerate(lines):
        # the ink, not the advance, starts at the gutter, as the site's -0.045em pulls the L back
        left, _ = A.ink(s, S, DISPLAY)
        body.append(text(s, G - left, first_base + i * lead, S, DISPLAY, c["on_bar"]))

    # the dots, centred under "stand" (the word, not its period), 0.12 em under the bar
    last = lines[-1]
    left, _ = A.ink(last, S, DISPLAY)
    x0 = G - left
    start = last.index("stand")
    sx = x0 + (A.width(last[:start], S, DISPLAY) + S * DISPLAY[2] if start else 0)
    l2, r2 = (sx + e for e in A.ink("stand", S, DISPLAY))
    dw = (1.12 if narrow else 0.72) * S
    dot_top = bar_bottom + 0.12 * S
    body.append(dots((l2 + r2) / 2, dot_top, dw, c["ink"]))

    H = dot_top + dw / 4 + (44 if narrow else 64)
    plate = f'<rect fill="{c["ground"]}" width="{W}" height="{num(H)}" rx="{num(10 * u)}"/>'
    clip = f'<clipPath id="p"><rect width="{W}" height="{num(H)}" rx="{num(10 * u)}"/></clipPath>'
    return svg(W, H, "stetdoc. Let it stand.", f'<g clip-path="url(#p)">{plate}{"".join(body)}</g>', clip)


# ---------- the benchmark: a table drawn on the stet bar, at true scale ----------


def track(c: dict, x: float, y: float, w: float, h: float, stet: int, fast: int, slow: int, label_size: float, u: float) -> str:
    """A groove cut into the bar from 0 to the row's slowest setup: stetdoc's bar lit court green at its true length,
    the other setups a red-tinted band with solid red ends, and the scale written at both ends under it."""
    out = [
        f'<rect fill="{c["groove"]}" fill-opacity="{c["groove_a"]:g}" x="{num(x)}" y="{num(y)}" width="{num(w)}" height="{num(h)}" rx="{num(4 * u)}"/>'
    ]
    # the groove's top shade, as a cut into the plane
    out.append(f'<rect fill="#000000" fill-opacity="0.18" x="{num(x + 8)}" y="{num(y)}" width="{num(w - 16)}" height="2"/>')
    inset = 6
    ih = h - 2 * inset
    bx0 = x + inset + (w - 2 * inset) * fast / slow
    bx1 = x + w - inset
    out.append(f'<rect fill="{c["red"]}" fill-opacity="{c["red_band_a"]:g}" x="{num(bx0)}" y="{num(y + inset)}" width="{num(bx1 - bx0)}" height="{num(ih)}" rx="{num(2 * u)}"/>')
    end = 3 * u
    for ex in (bx0, bx1 - end):
        out.append(f'<rect fill="{c["red"]}" x="{num(ex)}" y="{num(y + inset)}" width="{num(end)}" height="{num(ih)}" rx="{num(end / 2)}"/>')
    gw = (w - 2 * inset) * stet / slow
    out.append(f'<rect fill="{c["green"]}" x="{num(x + inset)}" y="{num(y + inset)}" width="{num(gw)}" height="{num(ih)}" rx="{num(2 * u)}"/>')
    # Clauseo's lit tile: a top highlight, and the shadow stays neutral
    out.append(f'<rect fill="#FFFFFF" fill-opacity="0.22" x="{num(x + inset + 4)}" y="{num(y + inset)}" width="{num(gw - 8)}" height="2"/>')
    ls = label_size * 0.92
    out.append(text("stetdoc", x + inset + 16, y + h / 2 + A.cap * ls / 2, ls, BODY, c["on_colour"]))
    under = y + h + 12 + A.cap * label_size
    out.append(text("0", x, under, label_size, LABEL, c["on_bar_quiet"], alpha=c["on_bar_quiet_a"]))
    out.append(text(f"slowest {slow} s", x + w, under, label_size, LABEL, c["on_bar_red"], anchor="end"))
    return "".join(out)


def key(c: dict, x: float, y: float, size: float, u: float, stacked: bool, right: float | None = None) -> str:
    """The key: both swatches and what they stand for. y is the first label's baseline."""
    sw, sh = size * 1.5, size * 0.8
    items = [("stetdoc", "green"), ("The other four setups, fastest to slowest", "red")]
    widths = [sw + size * 0.5 + A.width(t, size, BODY) for t, _ in items]
    gap = size * 1.4
    if right is not None and not stacked:
        x = right - (sum(widths) + gap)
    out = []
    for i, (t, kind) in enumerate(items):
        top = y - A.cap * size / 2 - sh / 2
        if kind == "green":
            out.append(f'<rect fill="{c["green"]}" x="{num(x)}" y="{num(top)}" width="{num(sw)}" height="{num(sh)}" rx="{num(2 * u)}"/>')
        else:
            out.append(f'<rect fill="{c["red"]}" fill-opacity="{c["red_band_a"]:g}" x="{num(x)}" y="{num(top)}" width="{num(sw)}" height="{num(sh)}" rx="{num(2 * u)}"/>')
            end = 3 * u
            out.append(f'<rect fill="{c["red"]}" x="{num(x)}" y="{num(top)}" width="{num(end)}" height="{num(sh)}" rx="{num(end / 2)}"/>')
            out.append(f'<rect fill="{c["red"]}" x="{num(x + sw - end)}" y="{num(top)}" width="{num(end)}" height="{num(sh)}" rx="{num(end / 2)}"/>')
        out.append(text(t, x + sw + size * 0.5, y, size, BODY, c["on_bar_quiet"], alpha=c["on_bar_quiet_a"]))
        if stacked:
            y += size * 1.6
        else:
            x += widths[i] + gap
    return "".join(out)


def bench(c: dict, narrow: bool) -> str:
    W = 700 if narrow else 1600
    u = U_NARROW if narrow else U_WIDE
    P = 36 if narrow else 72  # the plate's inner padding
    body = []
    cap_size = 34
    lab = 27 if narrow else 26
    y = (48 if narrow else 68) + A.cap * cap_size
    body.append(text("Median time per task", P, y, cap_size, TITLE, c["on_bar"]))
    if narrow:
        body.append(key(c, P, y + 44 + A.cap * lab, lab, u, stacked=True))
        y += 44 + lab * 1.6 * 2 + 24
    else:
        body.append(key(c, P, y, lab, u, stacked=False, right=W - P))
        y += 44
    fig = 80 if narrow else 84
    rng = 42 if narrow else 46
    model = 32 if narrow else 30
    for i, (name, stet, (fast, slow)) in enumerate(BENCH):
        body.append(f'<rect fill="{c["divide"]}" fill-opacity="{c["divide_a"]:g}" x="{P}" y="{num(y)}" width="{W - 2 * P}" height="2"/>')
        if narrow:
            # model over the two times, the track under them across the plate
            mb = y + 40 + A.cap * model
            body.append(text(name, P, mb, model, BODY, c["on_bar"]))
            lb = mb + 26 + A.cap * lab
            fb = lb + 16 + A.cap * fig
            ox = P + 0.47 * (W - 2 * P)
            body.append(text("stetdoc", P, lb, lab, BODY, c["on_bar_quiet"], alpha=c["on_bar_quiet_a"]))
            body.append(text("Other setups", ox, lb, lab, BODY, c["on_bar_quiet"], alpha=c["on_bar_quiet_a"]))
            body.append(text(f"{stet} s", P - 2, fb, fig, FIGURE, c["on_bar_green"]))
            body.append(text(f"{fast} to {slow} s", ox, fb, rng, RANGE, c["on_bar_red"]))
            ty = fb + 30
            body.append(track(c, P, ty, W - 2 * P, 48, stet, fast, slow, lab, u))
            y = ty + 48 + 12 + A.cap * lab + 40
        else:
            # one line: model, stetdoc's time, the track level with the figures' middle, the other setups' range
            top = y + 44
            lb = top + A.cap * lab
            fb = lb + 18 + A.cap * fig
            mid = fb - A.cap * fig / 2
            body.append(text(name, P, mid + A.cap * model / 2, model, BODY, c["on_bar"]))
            fx = P + max(A.width(m, model, BODY) for m, _, _ in BENCH) + 56
            fw = max(A.width(f"{s} s", fig, FIGURE) for _, s, _ in BENCH)
            rw = max(A.width(f"{a} to {b} s", rng, RANGE) for _, _, (a, b) in BENCH)
            tx = fx + fw + 48
            tw = W - P - rw - 48 - tx
            body.append(text("stetdoc", fx, lb, lab, BODY, c["on_bar_quiet"], alpha=c["on_bar_quiet_a"]))
            body.append(text(f"{stet} s", fx - 3, fb, fig, FIGURE, c["on_bar_green"]))
            body.append(track(c, tx, mid - 26, tw, 52, stet, fast, slow, lab, u))
            rx = W - P - rw
            body.append(text("Other setups", rx, lb, lab, BODY, c["on_bar_quiet"], alpha=c["on_bar_quiet_a"]))
            body.append(text(f"{fast} to {slow} s", rx - 3, fb, rng, RANGE, c["on_bar_red"]))
            y = max(fb, mid + 26 + 12 + A.cap * lab) + 44
    H = y + (8 if narrow else 16)
    # a bar that carries words takes 6 px corners
    plate = f'<rect fill="{c["bar"]}" width="{W}" height="{num(H)}" rx="{num(6 * u)}"/>'
    title = "Median time per task. " + " ".join(
        f"On {m}, stetdoc took {s} s and {'the other four setups' if i == 0 else 'the others'} {a} to {b} s."
        for i, (m, s, (a, b)) in enumerate(BENCH)
    )
    return svg(W, H, title, plate + "".join(body))


def main() -> None:
    for light, c in (("day", DAY), ("night", NIGHT)):
        for narrow in (False, True):
            tail = "-narrow" if narrow else ""
            A.begin()
            (HERE / f"banner-{light}{tail}.svg").write_text(banner(c, narrow))
            A.begin()
            (HERE / f"bench-{light}{tail}.svg").write_text(bench(c, narrow))


if __name__ == "__main__":
    main()
