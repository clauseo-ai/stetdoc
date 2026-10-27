#!/usr/bin/env bash
# Prints a Markdown summary of one evals/run.sh results directory: per case the score, the mean
# turns, tool calls, tokens, cost and wall time per run, and the graders that failed.
#   evals/summary.sh evals/results/<run>
set -euo pipefail
dir=${1:?usage: summary.sh RESULTS_DIR}
metrics="$dir/metrics.jsonl"
[ -f "$metrics" ] || : > "$metrics"
echo "# stetdoc eval summary: $(basename "$dir")"
echo
echo "| case | score | runs passed | turns | tool calls | tool errors | input tokens (cached) | output tokens | cost \$ | wall s | failed graders (runs) |"
echo "|---|---|---|---|---|---|---|---|---|---|---|"
for agg in "$dir"/*/aggregate-result.json; do
  [ -f "$agg" ] || continue
  jq -r --slurpfile m <(jq -s . "$metrics") '
    def mean(f): if length == 0 then 0 else (map(f // 0) | add / length) end;
    def r1: (. * 10 | round) / 10;
    .cases[] | . as $c
    | ($c.arms.with // []) as $runs
    | ($m[0] | map(select(.case == $c.name))) as $mm
    | [ $c.name,
        ($c.aggregates.score | r1 | tostring),
        "\($runs | map(select(.passed)) | length)/\($runs | length)",
        ($runs | mean(.turns) | r1 | tostring),
        ($mm | mean(.tool_calls) | r1 | tostring),
        ($mm | mean(.tool_errors) | r1 | tostring),
        "\($mm | mean(.input_tokens + .cache_creation_tokens + .cache_read_tokens) | round) (\($mm | mean(.cache_read_tokens) | round))",
        ($mm | mean(.output_tokens) | round | tostring),
        ($runs | mean(.costUsd) | . * 100 | round / 100 | tostring),
        ($runs | mean(.durationSeconds) | round | tostring),
        ([$runs[] | (.graders // [])[] | select(.passed | not) | .name]
          | group_by(.) | map("\(.[0]) (\(length))") | join(", ")),
        ([$runs[] | .error // empty] | if length > 0 then " errors: " + join("; ") else "" end)
      ]
    | "| " + (.[0:10] | join(" | ")) + " | " + .[10] + .[11] + " |"' "$agg"
done
echo
for agg in "$dir"/*/aggregate-result.json; do
  [ -f "$agg" ] || continue
  jq -r '"\(input_filename | split("/")[-2]): \(.aggregates.casesTotal) cases, overall score \(.aggregates.overallScore * 100 | round / 100), \(.durationSeconds) s, $\(.costUsd * 100 | round / 100)\(if .partial then " (partial: " + (.partialReason // "") + ")" else "" end)"' "$agg"
done
