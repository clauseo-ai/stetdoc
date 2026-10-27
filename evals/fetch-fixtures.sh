#!/usr/bin/env bash
# Puts the eval documents in evals/fixtures/, verified by SHA-256. They come from Harvey LAB
# (MIT), which corpus/sources.tsv marks as not vendorable, so they are fetched, never committed.
# It takes each one from the shared corpus cache when `cargo xtask corpus fetch` already put it
# there, and otherwise downloads it from the commit corpus/sources.tsv pins.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
dest="$here/fixtures"
mkdir -p "$dest"
cache="${STETDOC_CORPUS_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/stetdoc/corpora}"
commit=$(awk -F'\t' '$1 == "harvey-labs" { print $3 }' "$repo/corpus/sources.tsv")
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }
grep -v '^#' "$here/fixtures.tsv" | while IFS=$'\t' read -r name sha path; do
  out="$dest/$name.docx"
  if [ -f "$out" ] && [ "$(sha256 "$out")" = "$sha" ]; then echo "ok      $name"; continue; fi
  blob="$cache/blobs/${sha:0:2}/$sha"
  if [ -f "$blob" ]; then
    cp "$blob" "$out.part"; from=cache
  else
    curl -fsSL "https://raw.githubusercontent.com/harveyai/harvey-labs/$commit/$path" -o "$out.part"; from=github
  fi
  got=$(sha256 "$out.part")
  if [ "$got" != "$sha" ]; then rm -f "$out.part"; echo "error: $name from $from has SHA-256 $got, expected $sha" >&2; exit 1; fi
  mv "$out.part" "$out"; echo "fetched $name ($from)"
done
