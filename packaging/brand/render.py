"""Write stetdoc's mark and icons: the SVGs, the favicon, the touch and web icons and the Claude plugin's icon.

Run from the repository root:
    uv run --no-project --with resvg-py --with pillow python packaging/brand/render.py

The geometry and colours follow Clauseo's design language (DESIGN.md in clauseo-acp-private, "The mark"):
one flat glyph, square cuts and round dots, graphite by day and #ECEBE9 at night, and a full-bleed tile with
continuous corners for the icons. Brick is Clauseo's alone, so it never appears here.
"""

import io
from pathlib import Path

import resvg_py
from PIL import Image

HERE = Path(__file__).parent
ROOT = HERE.parent.parent

GRAPHITE = "#23201D"  # Clauseo's primary, and its mark in one colour
NIGHT = "#ECEBE9"  # primary text at night
TILE = "#3A3631"  # the icon's tile: graphite one step up, so it still parts from the night ground
WHITE = "#FFFFFF"

# The stet, on a 32-unit grid: the strike, square cut, over the proofreader's three dots.
# The bar is as thick as a dot is wide; dots sit half a dot apart; bar and dots three quarters of a dot apart.
W, H = 32, 22


def glyph(fill: str, k: float = 1.0, x: float = 0.0, y: float = 0.0, attr: str = "fill") -> str:
    def f(n: float) -> str:
        return f"{round(n, 2):g}"

    parts = [f'<rect {attr}="{fill}" x="{f(x)}" y="{f(y)}" width="{f(32 * k)}" height="{f(8 * k)}"/>']
    for cx in (4, 16, 28):
        parts.append(f'<circle {attr}="{fill}" cx="{f(x + cx * k)}" cy="{f(y + 18 * k)}" r="{f(4 * k)}"/>')
    return "".join(parts)


def squircle(x: float, y: float, s: float, r: float) -> str:
    """A rounded square with continuous corners, the curve Apple draws, as Clauseo's icon.mjs draws it."""
    X, Y = x + s, y + s
    a, b, c, d = 1.52866483 * r, 1.08849296 * r, 0.86840694 * r, 0.63149379 * r
    e, g, h = 0.07491139 * r, 0.37282383 * r, 0.16906001 * r

    def p(u: float, v: float) -> str:
        return f"{round(u, 2):g} {round(v, 2):g}"

    return (
        f"M {p(x + a, y)} L {p(X - a, y)}"
        f" C {p(X - b, y)} {p(X - c, y)} {p(X - d, y + e)} C {p(X - g, y + h)} {p(X - h, y + g)} {p(X - e, y + d)}"
        f" C {p(X, y + c)} {p(X, y + b)} {p(X, y + a)} L {p(X, Y - a)}"
        f" C {p(X, Y - b)} {p(X, Y - c)} {p(X - e, Y - d)} C {p(X - h, Y - g)} {p(X - g, Y - h)} {p(X - d, Y - e)}"
        f" C {p(X - c, Y)} {p(X - b, Y)} {p(X - a, Y)} L {p(x + a, Y)}"
        f" C {p(x + b, Y)} {p(x + c, Y)} {p(x + d, Y - e)} C {p(x + g, Y - h)} {p(x + h, Y - g)} {p(x + e, Y - d)}"
        f" C {p(x, Y - c)} {p(x, Y - b)} {p(x, Y - a)} L {p(x, y + a)}"
        f" C {p(x, y + b)} {p(x, y + c)} {p(x + e, y + d)} C {p(x + h, y + g)} {p(x + g, y + h)} {p(x + d, y + e)}"
        f" C {p(x + c, y)} {p(x + b, y)} {p(x + a, y)} Z"
    )


def svg(view: str, body: str, title: str = "stetdoc") -> str:
    return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view}"><title>{title}</title>{body}</svg>\n'


def tile_art(S: int, square: bool = False) -> str:
    """The glyph in white on the graphite tile, filling an S square. The glyph is 60% of the tile wide and sits a
    little low of centre, because the solid bar outweighs the dots."""
    k = 0.6 * S / W
    x = (S - W * k) / 2
    # centre a point 40% of the way from the box centre toward the area centroid, as Clauseo places its comma
    bar, dots = 32 * 8, 3 * 3.14159 * 16
    cy = (4 * bar + 18 * dots) / (bar + dots)
    y = S / 2 - (H / 2 + 0.4 * (cy - H / 2)) * k
    back = f'<rect width="{S}" height="{S}" fill="{TILE}"/>' if square else f'<path fill="{TILE}" d="{squircle(0, 0, S, S * 0.18)}"/>'
    return svg(f"0 0 {S} {S}", back + glyph(WHITE, k, x, y))


def render(text: str, size: int, height: int | None = None) -> Image.Image:
    png = bytes(resvg_py.svg_to_bytes(svg_string=text, width=size, height=height or size))
    return Image.open(io.BytesIO(png)).convert("RGBA")


def small_tile(S: int) -> Image.Image:
    """The 16 and 32 px icons, drawn to the pixel: the tile rendered, the glyph set as whole pixels on it."""
    im = render(svg(f"0 0 {S} {S}", f'<path fill="{TILE}" d="{squircle(0, 0, S, S * 0.18)}"/>'), S)
    px = im.load()
    white = (255, 255, 255, 255)
    if S == 16:  # bar 10 by 2, dots 2 by 2
        cells = [(x, y) for x in range(3, 13) for y in (5, 6)]
        cells += [(x, y) for x0 in (3, 7, 11) for x in (x0, x0 + 1) for y in (9, 10)]
        for c in cells:
            px[c] = white
        return im
    # 32: bar 18 by 4, dots 4 px across with their corners at half, 3 px between bar and dots
    for x in range(7, 25):
        for y in range(10, 14):
            px[x, y] = white
    for x0 in (7, 14, 21):
        for dx in range(4):
            for dy in range(4):
                corner = dx in (0, 3) and dy in (0, 3)
                r, g, b, _ = px[x0 + dx, 17 + dy]
                px[x0 + dx, 17 + dy] = ((r + 255) // 2, (g + 255) // 2, (b + 255) // 2, 255) if corner else white
    return im


def main() -> None:
    view = f"0 {-(32 - H) / 2:g} {W} 32"
    favicon = svg(view, f"<style>*{{fill:{GRAPHITE}}}@media (prefers-color-scheme:dark){{*{{fill:{NIGHT}}}}}</style>" + glyph("", attr="class"))
    files = {
        "favicon.svg": favicon.replace(' class=""', ""),
        "mark-graphite.svg": svg(f"0 0 {W} {H}", glyph(GRAPHITE)),
        "mark-night.svg": svg(f"0 0 {W} {H}", glyph(NIGHT)),
        "mark-white.svg": svg(f"0 0 {W} {H}", glyph(WHITE)),
        "icon.svg": tile_art(1024),
    }
    for name, text in files.items():
        (HERE / name).write_text(text)
    # a copy at the repository root, the first place tools such as boxbox look for a repository's icon
    (ROOT / "favicon.svg").write_text(files["favicon.svg"])

    frames = [small_tile(16), small_tile(32), render(tile_art(1024), 48)]
    # Pillow builds the .ico from the largest frame and uses an appended frame where its size matches.
    frames[-1].save(HERE / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)], append_images=frames[:-1])
    render(tile_art(180, square=True), 180).convert("RGB").save(HERE / "apple-touch-icon.png", optimize=True)
    for s in (192, 512):
        render(tile_art(1024), s).save(HERE / f"icon-{s}.png", optimize=True)
    render(tile_art(1024), 1024).save(ROOT / "plugins/claude/.claude-plugin/icon.png", optimize=True)


if __name__ == "__main__":
    main()
