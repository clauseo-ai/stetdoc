#!/usr/bin/env bash
# Full-page WebKit screenshots of one URL, day and night, at each width.
#
# Usage: site/scripts/webkit-shots.sh BASE_URL OUTDIR [widths...]
#   e.g. site/scripts/webkit-shots.sh http://127.0.0.1:3210/ /tmp/shots 390 1440
# Default widths: 320 390 430 768 1024 1280 1440 1600. Writes OUTDIR/webkit-{width}-{day|night}.png.
# The Swift helper is compiled once to /tmp/webkit-shot and rebuilt when webkit-shot.swift is newer.
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: webkit-shots.sh BASE_URL OUTDIR [widths...]" >&2
  exit 2
fi

base="$1"
outdir="$2"
shift 2
if [ "$#" -gt 0 ]; then
  widths=("$@")
else
  widths=(320 390 430 768 1024 1280 1440 1600)
fi

here="$(cd "$(dirname "$0")" && pwd)"
src="$here/webkit-shot.swift"
bin=/tmp/webkit-shot

if [ ! -x "$bin" ] || [ "$src" -nt "$bin" ]; then
  swiftc -O "$src" -o "$bin"
fi

mkdir -p "$outdir"
for width in "${widths[@]}"; do
  for look in day night; do
    if [ "$look" = night ]; then
      "$bin" --url "$base" --width "$width" --full --dark --out "$outdir/webkit-$width-$look.png"
    else
      "$bin" --url "$base" --width "$width" --full --out "$outdir/webkit-$width-$look.png"
    fi
  done
done
