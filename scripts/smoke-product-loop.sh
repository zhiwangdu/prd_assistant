#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
API_KEY="${LOGAGENT_NATIVE_API_KEY:-dev-token}"
URL="http://127.0.0.1:50999"
DATA_DIR="/tmp/logagent-server-influxql-tool"
LOG_FILE="/tmp/logagent-product-loop-smoke.log"

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$name" >&2
    exit 1
  fi
}

api() {
  curl --silent --show-error --fail-with-body \
    -H "Authorization: Bearer ${API_KEY}" \
    "$@"
}

wait_for_task() {
  local task_id="$1"
  local status=""
  for _ in {1..120}; do
    status="$(api "${URL}/api/tasks/${task_id}" | jq -r '.status')"
    case "$status" in
      SUCCEEDED)
        return 0
        ;;
      FAILED)
        api "${URL}/api/tasks/${task_id}" | jq .
        printf 'Task failed: %s\n' "$task_id" >&2
        return 1
        ;;
    esac
    sleep 1
  done
  printf 'Timed out waiting for task %s, last status=%s\n' "$task_id" "$status" >&2
  return 1
}

require_command cargo
require_command curl
require_command go
require_command jq

cd "$ROOT_DIR"

rm -rf "$DATA_DIR"
tmp_dir="$(mktemp -d /tmp/logagent-product-loop.XXXXXX)"
cleanup() {
  if [[ -n "${server_pid:-}" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

cat >"${tmp_dir}/queries.jsonl" <<'EOF'
{"timestamp":"2026-06-08T00:00:00Z","query":"select * from cpu limit 100000"}
{"timestamp":"2026-06-08T00:00:01Z","query":"show series from cpu"}
EOF

printf 'Building InfluxQL analyzer...\n'
"$ROOT_DIR/scripts/build-tools.sh" --only influxql --output-dir "$ROOT_DIR/target/tools" >/dev/null

printf 'Building server...\n'
cargo build -p logagent-server >/dev/null

printf 'Starting smoke server on %s...\n' "$URL"
LOGAGENT_NATIVE_API_KEY="$API_KEY" \
  LOGAGENT_TOOL_INFLUXQL_ANALYZER="$ROOT_DIR/target/tools/influxql-analyzer" \
  target/debug/logagent-server --config examples/server-influxql-tool.yaml >"$LOG_FILE" 2>&1 &
server_pid=$!

for _ in {1..30}; do
  if curl --max-time 1 --silent --fail "${URL}/health" >/dev/null; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    printf 'Server exited during startup. See %s\n' "$LOG_FILE" >&2
    exit 1
  fi
  sleep 1
done
curl --max-time 1 --silent --fail "${URL}/health" >/dev/null

printf 'Uploading InfluxQL JSONL fixture...\n'
upload_id="$(
  api -X POST \
    -F "filename=queries.jsonl" \
    -F "file=@${tmp_dir}/queries.jsonl" \
    "${URL}/api/uploads" | jq -r '.uploadId'
)"

printf 'Creating first analysis task...\n'
task_id="$(
  jq -n --arg uploadId "$upload_id" --arg question "influxql no time filter large limit" \
    '{uploadId:$uploadId, question:$question}' |
    api -X POST -H 'Content-Type: application/json' --data-binary @- "${URL}/api/tasks" |
    jq -r '.taskId'
)"
wait_for_task "$task_id"

artifacts="$(api "${URL}/api/tasks/${task_id}/artifacts")"
tool_findings="$(jq '[.toolResults[]?.findings[]?] | length' <<<"$artifacts")"
if ((tool_findings < 1)); then
  printf 'Expected at least one Tool Runner finding\n' >&2
  jq . <<<"$artifacts"
  exit 1
fi

printf 'Confirming first task as Case...\n'
case_id="$(
  jq -n \
    --arg title "InfluxQL query lacks bounded time filter" \
    --arg symptom "InfluxQL JSONL contains unbounded query patterns" \
    --arg rootCause "missing time filter or risky meta query pattern" \
    --arg solution "add bounded time predicate and review large LIMIT / meta queries" \
    '{title:$title, symptom:$symptom, rootCause:$rootCause, solution:$solution}' |
    api -X POST -H 'Content-Type: application/json' --data-binary @- "${URL}/api/tasks/${task_id}/case" |
    jq -r '.case.caseId'
)"

printf 'Creating second task to verify Case recall...\n'
recall_task_id="$(
  jq -n --arg uploadId "$upload_id" --arg question "influxql missing time filter regression" \
    '{uploadId:$uploadId, question:$question}' |
    api -X POST -H 'Content-Type: application/json' --data-binary @- "${URL}/api/tasks" |
    jq -r '.taskId'
)"
wait_for_task "$recall_task_id"

recall_artifacts="$(api "${URL}/api/tasks/${recall_task_id}/artifacts")"
recalled_case_id="$(jq -r '.caseContext.cases[0].caseId // empty' <<<"$recall_artifacts")"
if [[ "$recalled_case_id" != "$case_id" ]]; then
  printf 'Expected recalled case %s, got %s\n' "$case_id" "${recalled_case_id:-<empty>}" >&2
  jq '.caseContext' <<<"$recall_artifacts"
  exit 1
fi

printf 'Product loop smoke passed: upload=%s task=%s case=%s recallTask=%s\n' \
  "$upload_id" "$task_id" "$case_id" "$recall_task_id"
