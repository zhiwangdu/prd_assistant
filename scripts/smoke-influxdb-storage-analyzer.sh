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
"$ROOT_DIR/scripts/build-tools.sh" --only influxdb --output-dir "$ROOT_DIR/target/tools" >/dev/null

tool="$ROOT_DIR/target/tools/influxdb_storage_analyzer"
if [[ ! -x "$tool" ]]; then
  printf 'InfluxDB storage analyzer was not built: %s\n' "$tool" >&2
  exit 1
fi

tmp_dir="$(mktemp -d /tmp/logagent-influxdb-storage-smoke.XXXXXX)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

printf 'not a real tsm file' >"$tmp_dir/bad.tsm"

if "$tool" -input "$tmp_dir/bad.tsm" -kind auto >"$tmp_dir/report.json" 2>"$tmp_dir/stderr.txt"; then
  printf 'Expected analyzer to return non-zero for invalid TSM fixture\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi

if ! grep -q '"tool": "influxdb_storage_analyzer"' "$tmp_dir/report.json"; then
  printf 'Expected influxdb_storage_analyzer tool id in JSON output\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi
if ! grep -q '"severity": "high"' "$tmp_dir/report.json"; then
  printf 'Expected high severity finding in JSON output\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi

printf 'InfluxDB storage analyzer smoke passed: %s\n' "$tool"
