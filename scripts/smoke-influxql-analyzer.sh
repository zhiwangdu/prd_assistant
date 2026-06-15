#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$name" >&2
    exit 1
  fi
}

require_command go

cd "$ROOT_DIR"
"$ROOT_DIR/scripts/build-tools.sh" --only influxql --output-dir "$ROOT_DIR/target/tools" >/dev/null

tmp_dir="$(mktemp -d /tmp/logagent-influxql-smoke.XXXXXX)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

cat >"$tmp_dir/queries.jsonl" <<'EOF'
{"timestamp":"2026-06-08T00:00:00Z","query":"select * from cpu limit 100000"}
{"timestamp":"2026-06-08T00:00:01Z","query":"show series from cpu"}
EOF

"$ROOT_DIR/target/tools/influxql-analyzer" \
  -input "$tmp_dir/queries.jsonl" \
  -output json \
  -detail-limit 2 \
  >"$tmp_dir/report.json" \
  2>"$tmp_dir/progress.log"

if ! grep -q '"total_records": 2' "$tmp_dir/report.json"; then
  printf 'Expected total_records in analyzer JSON output\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi
if ! grep -q '"rule": "large_limit"' "$tmp_dir/report.json"; then
  printf 'Expected large_limit rule in analyzer JSON output\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi
if ! grep -q 'done: analyzed 2/2 records' "$tmp_dir/progress.log"; then
  printf 'Expected progress output on stderr\n' >&2
  cat "$tmp_dir/progress.log" >&2
  exit 1
fi

printf 'InfluxQL analyzer smoke passed: %s\n' "$ROOT_DIR/target/tools/influxql-analyzer"
