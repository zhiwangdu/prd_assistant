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
"$ROOT_DIR/scripts/build-tools.sh" --only opengemini --output-dir "$ROOT_DIR/target/tools" >/dev/null

tool="$ROOT_DIR/target/tools/opengemini-storage-analyzer"
if [[ ! -x "$tool" ]]; then
  printf 'openGemini storage analyzer was not built: %s\n' "$tool" >&2
  printf 'Set LOGAGENT_OPENGEMINI_SRC_DIR or place openGemini next to this repository.\n' >&2
  exit 1
fi

tmp_dir="$(mktemp -d /tmp/logagent-opengemini-storage-smoke.XXXXXX)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

part_dir="$tmp_dir/10_2_0000000000000001"
mkdir -p "$part_dir"
cat >"$part_dir/metadata.json" <<'JSON'
{"ItemsCount":9,"BlocksCount":2,"FirstItem":"","LastItem":"abcd"}
JSON
printf 'idx' >"$part_dir/index.bin"

"$tool" \
  --input "$part_dir/metadata.json" \
  --format json \
  >"$tmp_dir/report.json"

if ! grep -q '"tool":"opengemini_storage_analyzer"' "$tmp_dir/report.json"; then
  printf 'Expected opengemini_storage_analyzer tool id in JSON output\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi
if ! grep -q 'metadata ItemsCount 9 does not match directory item count 10' "$tmp_dir/report.json"; then
  printf 'Expected mergeset count mismatch finding in JSON output\n' >&2
  cat "$tmp_dir/report.json" >&2
  exit 1
fi

printf 'openGemini storage analyzer smoke passed: %s\n' "$tool"
