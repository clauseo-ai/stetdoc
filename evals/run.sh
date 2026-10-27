#!/usr/bin/env bash
# Runs the stetdoc plugin eval suite with Claude Code's `claude plugin eval` and writes a summary.
# See docs/evals.md. Every run is a real model call on your account.
#
#   evals/run.sh [--arm stetdoc|none|DIR] [--runs N] [--case GLOB] [--model M] [--judge-model M]
#                [--max-cost-usd USD] [--concurrency N] [--skip-without-stetdoc] [-- other eval options]
#
# --arm picks the plugin under test: stetdoc (plugins/claude, the default), none (an empty plugin,
# so the agent has only what is on PATH), or DIR, any plugin directory to compare against.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/.." && pwd)
arm=stetdoc runs=3 cases= skip_without=no extra=()
model=claude-opus-5-5 judge=claude-sonnet-5-5 ceiling= concurrency=1
while [ $# -gt 0 ]; do
  case "$1" in
    --arm) arm=$2; shift 2 ;;
    --runs) runs=$2; shift 2 ;;
    --case) cases=$2; shift 2 ;;
    --model) model=$2; shift 2 ;;
    --judge-model) judge=$2; shift 2 ;;
    --max-cost-usd) ceiling=$2; shift 2 ;;
    --concurrency|-j) concurrency=$2; shift 2 ;;
    --skip-without-stetdoc) skip_without=yes; shift ;;
    --) shift; extra=("$@"); break ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown option $1 (see --help)" >&2; exit 2 ;;
  esac
done

case "$arm" in
  stetdoc) target=../plugins/claude ;;
  none) target=arms/none ;;
  *) target=$(cd "$arm" && pwd) ;;
esac
# evals/arm is the plugin every case loads. Point it at the arm for this run, and back afterwards.
ln -sfn "$target" "$here/arm"
trap 'ln -sfn ../plugins/claude "$here/arm"' EXIT

"$here/fetch-fixtures.sh" >/dev/null
claude=$(command -v claude)
out="$here/results/$(date -u +%Y-%m-%dT%H-%M-%SZ)-$(basename "$arm")"
mkdir -p "$out"
export EVAL_STETDOC_METRICS="$out/metrics.jsonl"
# Each run gets a temporary home, so Vertex AI's application default credentials need their path.
adc="$HOME/.config/gcloud/application_default_credentials.json"
if [ -z "${GOOGLE_APPLICATION_CREDENTIALS:-}" ] && [ -f "$adc" ]; then export GOOGLE_APPLICATION_CREDENTIALS="$adc"; fi

common=(--scaffold --trust-plugin --ablation none --runs "$runs" --model "$model" --judge-model "$judge"
  --concurrency "$concurrency" --no-publish
  --allow-real-servers --allow-tools Bash Edit Write "mcp__plugin_stetdoc_stetdoc__*")
[ -n "$ceiling" ] && common+=(--max-cost-usd "$ceiling")
[ -n "$cases" ] && common+=(--case "$cases")

status=0
cd "$repo"
"$claude" plugin eval . --eval-dir evals/cases --output-dir "$out/cases" "${common[@]}" ${extra[@]+"${extra[@]}"} || status=$?

if [ "$skip_without" = no ]; then
  # The same suite runner, with every PATH entry that holds a stetdoc command removed, so neither
  # the agent nor the plugin's MCP server can find it.
  path=$(tr ':' '\n' <<<"$PATH" | while read -r d; do [ -n "$d" ] && [ ! -x "$d/stetdoc" ] && echo "$d"; done | paste -sd: -)
  PATH="$path" "$claude" plugin eval . --eval-dir evals/without-stetdoc --output-dir "$out/without-stetdoc" \
    "${common[@]}" ${extra[@]+"${extra[@]}"} || status=$?
fi

"$here/summary.sh" "$out" | tee "$out/summary.md"
exit "$status"
