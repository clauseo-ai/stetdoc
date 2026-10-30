# stetdoc's mark

stetdoc's mark is the proofreader's stet: a strike with three dots under it. On a proof the dots mean "let it stand", keep the struck word as it was. stetdoc keeps the same promise for a Word file. An edit becomes a tracked change someone can reject, and a file nobody edited comes back byte for byte.

It is drawn in Clauseo's design language, as a sibling of Clauseo's comma (`design/DESIGN.md`, "The mark", in clauseo-acp-private). Both marks are punctuation, cut flat in one colour: square ends on the bar, round dots, no outline, no effect. The bar is as thick as a dot is wide.

## Colour

- **Graphite** `#23201D` by day, Clauseo's primary and its one-colour mark.
- **`#ECEBE9`** at night, Clauseo's primary text at night.
- **The tile** is `#3A3631` with a white glyph. That is graphite one step up, so the tile still stands off Clauseo's night ground (`#1B1A18`). Where Clauseo's icon is a white comma on a brick tile, stetdoc's is a white stet on a graphite tile.
- **No brick.** Brick means Clauseo itself, so stetdoc never uses it. It doesn't use the court colours either, because in Clauseo they only ever carry a meaning.

## Files

| File | Use |
| --- | --- |
| `favicon.svg` | The mark alone, as Clauseo's favicon is. Graphite, and `#ECEBE9` when the system is dark. |
| `favicon.ico` | The tile at 16, 32 and 48 px. The 16 and 32 px sizes are set pixel by pixel. |
| `icon.svg` | The tile filling its square, with continuous corners, for Windows, the web and plugin listings. |
| `apple-touch-icon.png` | 180 px, square, opaque. iOS rounds the corners. |
| `icon-192.png`, `icon-512.png` | Web manifest icons. |
| `mark-graphite.svg`, `mark-night.svg`, `mark-white.svg` | The mark alone in each colour, cropped tight. |

`render.py` writes all of these, plus two copies outside this folder: `favicon.svg` at the repository root, the first place tools such as boxbox look for a repository's icon, and the Claude plugin's `plugins/claude/.claude-plugin/icon.png` (1024 px). To change anything, edit `render.py` and run it from the repository root:

```sh
uv run --no-project --with resvg-py --with pillow python packaging/brand/render.py
```

To use the icons on a page:

```html
<link rel="icon" href="/favicon.ico" sizes="32x32">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
```
