#!/usr/bin/env bash
# Stop hook of the eval harness plugin. When the agent under test stops, it writes
# .stetdoc-eval-report.txt in the workspace: what changed since the scaffold's snapshot, and for
# every .docx the agent created or changed, stetdoc's own check, revision ledger, comments and final
# text. The eval graders are regexes over this file, so a verdict never depends on what the agent
# says it did. From the session transcript the hook input names, it also counts turns' tool calls,
# tool errors and tokens, writes them to the report, and appends them with the case name to the
# file EVAL_STETDOC_METRICS names (evals/run.sh sets it), so the suite doubles as a benchmark.
set -uo pipefail
input=$(cat)
cd "${CLAUDE_PROJECT_DIR:-$PWD}" || exit 0
state="$HOME/.stetdoc-eval"
report=.stetdoc-eval-report.txt
[ -f "$state/before.sha" ] || exit 0
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }
have_stetdoc=no
command -v stetdoc >/dev/null 2>&1 && have_stetdoc=yes

# findings FILE: every finding of stetdoc check, counted by code and part.
findings() { stetdoc check "$1" --all 2>&1 | sed -n 's/^  \([a-z_]*\) (\([^) ]*\).*/\1 \2/p' | sort | uniq -c | sed 's/^ */finding: /'; }
# class_a FILE: the number of check findings Word would reject or repair (all but the lints below).
class_a() { stetdoc check "$1" --all 2>&1 | grep -E '^  [a-z_]+ \(' | grep -vcE '^  (schema_non_canonical_order|empty_properties|para_id_missing) \(' || true; }
# violations FILE: the number stetdoc check reports for FILE, or -1 when check fails.
violations() {
  local out
  out=$(stetdoc check "$1" 2>&1) || { echo -1; return; }
  if grep -q '^Validation: no violations' <<<"$out"; then echo 0; else
    sed -n 's/^Validation: \([0-9][0-9]*\) violation.*/\1/p' <<<"$out" | head -1
  fi
}

# metrics: one JSON object from the transcript, or {} without jq or a transcript.
metrics() {
  local t
  t=$(jq -r '.transcript_path // empty' <<<"$input" 2>/dev/null)
  if [ -z "$t" ] || [ ! -f "$t" ]; then echo '{}'; return; fi
  jq -cs --arg case "$(cat "$state/case" 2>/dev/null)" '
    [.[] | select(.type == "assistant")] as $a
    | [$a[] | .message.content[]? | select(type == "object" and .type == "tool_use")] as $calls
    | ($a | map(select(.message.id != null)) | unique_by(.message.id) | map(.message.usage // {})) as $u
    | { case: $case,
        tool_calls: ($calls | length),
        by_tool: ($calls | map(.name) | group_by(.) | map({(.[0]): length}) | add // {}),
        tool_errors: ([.[] | select(.type == "user") | .message.content[]?
                       | select(type == "object" and .type == "tool_result" and .is_error == true)] | length),
        skills: ($calls | map(select(.name == "Skill") | .input.skill)),
        input_tokens: ($u | map(.input_tokens // 0) | add // 0),
        cache_creation_tokens: ($u | map(.cache_creation_input_tokens // 0) | add // 0),
        cache_read_tokens: ($u | map(.cache_read_input_tokens // 0) | add // 0),
        output_tokens: ($u | map(.output_tokens // 0) | add // 0) }' "$t" 2>/dev/null || echo '{}'
}
m=$(metrics)
if [ -n "${EVAL_STETDOC_METRICS:-}" ]; then echo "$m" >> "$EVAL_STETDOC_METRICS"; fi

{
  echo "stetdoc-eval-report 1"
  echo "metrics: $m"
  echo "stetdoc-on-path: $have_stetdoc"
  worst=0 worst_a=0
  while read -r sum path; do
    if [ ! -e "$path" ]; then echo "deleted: $path"
    elif [ "$(sha256 "$path")" = "$sum" ]; then echo "unchanged: $path"
    else echo "changed: $path"; fi
    if [ "$have_stetdoc" = yes ] && [[ "$path" == *.docx ]]; then
      v=$(violations "$path"); [ "${v:-0}" -gt "$worst" ] && worst=$v
      a=$(class_a "$path"); [ "${a:-0}" -gt "$worst_a" ] && worst_a=$a
    fi
  done < "$state/before.sha"
  # A working copy (NAME.stet/) counts once, as a leftover, not file by file.
  find . -type d -name '*.stet' -prune -o -type f ! -name "$report" -print | sort | while read -r f; do
    grep -q "  $f\$" "$state/before.sha" || echo "created: $f"
  done
  find . -type d -name '*.stet' -prune -print | sort | sed 's/^/leftover-working-copy: /'
  [ "$have_stetdoc" = yes ] || exit 0
  find . -type d -name '*.stet' -prune -o -type f -name '*.docx' -print | sort | while read -r f; do
    before=$(grep "  $f\$" "$state/before.sha" | cut -d' ' -f1)
    [ -n "$before" ] && [ "$before" = "$(sha256 "$f")" ] && continue
    echo
    echo "== $f"
    v=$(violations "$f"); v=${v:--1}
    # Valid: no more findings that Word would reject or repair than the inputs had. The lints Word accepts
    # (a non-canonical element order, an empty property element, a missing paragraph id) do not count.
    a=$(class_a "$f")
    if [ "$v" -ge 0 ] && [ "${a:-0}" -le "$worst_a" ]; then echo "valid: yes"; else echo "valid: no"; fi
    echo "violations: $v (inputs at most $worst)"
    echo "-- check"; findings "$f"
    ledger=$(stetdoc view "$f" --revisions --full --all 2>&1)
    n=$(sed -n 's/^{summary: \([0-9][0-9]*\) tracked change.*/\1/p' <<<"$ledger" | head -1)
    echo "remaining-revisions: ${n:-0}"
    echo "-- revisions"; echo "$ledger"
    echo "-- comments"; stetdoc view "$f" --comments --all 2>&1
    echo "-- final text"; stetdoc view "$f" --mode final --all 2>&1
  done
} > "$report" 2>&1
# Keep the report and the session transcript next to the metrics, as evidence for each run.
if [ -n "${EVAL_STETDOC_METRICS:-}" ]; then
  keep="$(dirname "$EVAL_STETDOC_METRICS")/reports"; mkdir -p "$keep"
  name="$(cat "$state/case" 2>/dev/null)-$(basename "${PWD%/home/cwd}")"
  cp "$report" "$keep/$name.txt" 2>/dev/null
  t=$(jq -r '.transcript_path // empty' <<<"$input" 2>/dev/null)
  [ -n "$t" ] && [ -f "$t" ] && cp "$t" "$keep/$name.transcript.jsonl" 2>/dev/null
fi
exit 0
