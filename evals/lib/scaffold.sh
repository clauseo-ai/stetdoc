#!/usr/bin/env bash
# Shared scaffold: each case's scaffold.sh sources this file and calls `setup NAME...`.
# It runs in the run's empty workspace with HOME set to the run's temporary home.
set -euo pipefail
evals_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
state="$HOME/.stetdoc-eval"
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }

# setup NAME...: copies each fixture to docs/NAME.docx, checks its SHA-256, saves the
# revision author a user would have saved with `stetdoc init`, and records the starting state.
setup() {
  mkdir -p docs "$state" "$HOME/.config/stetdoc"
  for name in "$@"; do
    local want src
    want=$(awk -F'\t' -v n="$name" '$1 == n { print $2 }' "$evals_dir/fixtures.tsv")
    src="$evals_dir/fixtures/$name.docx"
    if [ ! -f "$src" ]; then echo "missing $src: run evals/fetch-fixtures.sh first" >&2; exit 1; fi
    if [ "$(sha256 "$src")" != "$want" ]; then echo "$src does not match fixtures.tsv: run evals/fetch-fixtures.sh" >&2; exit 1; fi
    cp "$src" "docs/$name.docx"
    # The corpus cache stores documents read-only; a user's documents are writable.
    chmod u+w "docs/$name.docx"
  done
  printf 'author = "Eval Reviewer"\n' > "$HOME/.config/stetdoc/config.toml"
  basename "$(cd "$(dirname "$0")" && pwd)" > "$state/case"
}

# snapshot: records every file and its SHA-256, so the report can tell what the agent changed.
# Call it last, after any extra setup the case does.
snapshot() {
  find . -type f ! -path './.stetdoc-eval*' | sort | while read -r f; do echo "$(sha256 "$f")  $f"; done > "$state/before.sha"
}
