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

cat >"$tmp_dir/baseline.jsonl" <<'EOF'
{"timestamp":"2026-06-08T00:00:00Z","query":"select usage from cpu where time > now() - 1m"}
{"timestamp":"2026-06-08T00:00:10Z","query":"select usage from cpu where time > now() - 1m"}
EOF

cat >"$tmp_dir/candidate.jsonl" <<'EOF'
{"timestamp":"2026-06-08T00:00:00Z","query":"select usage from cpu where time > now() - 1m"}
{"timestamp":"2026-06-08T00:00:10Z","query":"select usage from cpu where time > now() - 1m"}
{"timestamp":"2026-06-08T00:00:20Z","query":"select * from cpu limit 100000"}
{"timestamp":"2026-06-08T00:00:30Z","query":"show series from cpu"}
EOF

"$ROOT_DIR/target/tools/influxql-analyzer" \
  -input-a "$tmp_dir/baseline.jsonl" \
  -input-b "$tmp_dir/candidate.jsonl" \
  -output json \
  -detail-limit 3 \
  -window-start 2026-06-08T00:00:00Z \
  -window-end 2026-06-08T00:01:00Z \
  >"$tmp_dir/compare.json" \
  2>"$tmp_dir/compare-progress.log"

if ! grep -q '"statement_delta": 2' "$tmp_dir/compare.json"; then
  printf 'Expected statement_delta in analyzer compare JSON output\n' >&2
  cat "$tmp_dir/compare.json" >&2
  exit 1
fi
if ! grep -q '"status": "added"' "$tmp_dir/compare.json"; then
  printf 'Expected added fingerprint in analyzer compare JSON output\n' >&2
  cat "$tmp_dir/compare.json" >&2
  exit 1
fi
if ! grep -q '"rule": "large_limit"' "$tmp_dir/compare.json"; then
  printf 'Expected large_limit rule delta in analyzer compare JSON output\n' >&2
  cat "$tmp_dir/compare.json" >&2
  exit 1
fi
if ! grep -q '\[A\] done: analyzed 2/2 records' "$tmp_dir/compare-progress.log"; then
  printf 'Expected A-side compare progress output on stderr\n' >&2
  cat "$tmp_dir/compare-progress.log" >&2
  exit 1
fi
if ! grep -q '\[B\] done: analyzed 4/4 records' "$tmp_dir/compare-progress.log"; then
  printf 'Expected B-side compare progress output on stderr\n' >&2
  cat "$tmp_dir/compare-progress.log" >&2
  exit 1
fi

printf 'InfluxQL analyzer report and compare smoke passed: %s\n' "$ROOT_DIR/target/tools/influxql-analyzer"
