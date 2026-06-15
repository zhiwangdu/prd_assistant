#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-logagent-workdir.sh
source "$SCRIPT_DIR/lib-logagent-workdir.sh"

FORCE=false

usage() {
  cat <<'EOF'
Usage: scripts/init-workdir.sh [--force]

Initializes $LOGAGENT_WORK_DIR for local Server runtime files.

Environment:
  LOGAGENT_WORK_DIR            Required. Runtime work directory.
  LOGAGENT_SERVER_BIND         Optional. Default: 127.0.0.1:50992
  LOGAGENT_PUBLIC_BASE_URL     Optional. Default: http://127.0.0.1:50992

The generated config uses storage.data_dir=$LOGAGENT_WORK_DIR/data and
auth.api_keys[].value_env=LOGAGENT_NATIVE_API_KEY.
EOF
}

while (($# > 0)); do
  case "$1" in
    --force)
      FORCE=true
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

work_dir="$(logagent_require_work_dir)"
logagent_prepare_work_dir "$work_dir"

bind="${LOGAGENT_SERVER_BIND:-127.0.0.1:50992}"
public_base_url="${LOGAGENT_PUBLIC_BASE_URL:-http://127.0.0.1:50992}"
config_path="$(logagent_server_config "$work_dir")"

if [[ -f "$config_path" && "$FORCE" != "true" ]]; then
  printf 'Work directory already has config: %s\n' "$config_path"
  printf 'Use --force to overwrite it.\n' >&2
  exit 1
fi

mkdir -p "$(dirname "$config_path")"
cat >"$config_path" <<EOF
server:
  bind: "${bind}"
  public_base_url: "${public_base_url}"
  max_concurrent_tasks: 2

storage:
  data_dir: "${work_dir}/data"
  max_upload_bytes: 2147483648
  max_chunk_bytes: 524288

auth:
  api_keys:
    - name: "native-agent"
      value_env: "LOGAGENT_NATIVE_API_KEY"

log_analyzer:
  max_matches: 200
  keywords:
    - error
    - exception
    - timeout
    - fail
    - failed
    - panic
    - fatal
    - refused
    - denied
    - verify
    - flux
    - influxql
    - "\"query\""
    - duration_ms
    - select
    - show
    - query
    - large_limit
    - no_time_filter

tools:
  flux_query_analyzer:
    enabled: true
    path: "${work_dir}/bin/tools/flux_query_analyzer"
    timeout_seconds: 30
    max_output_bytes: 1048576
    max_input_files: 3
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"
      - "--top-k"
      - "20"
      - "--max-input-lines"
      - "100000"
      - "--max-error-findings"
      - "20"
    match:
      file_patterns:
        - "*.jsonl"
        - "*.ndjson"
      keywords:
        - "flux"
        - "\"query\""
        - "duration_ms"

  influxql_analyzer:
    enabled: true
    path: "${work_dir}/bin/tools/influxql-analyzer"
    timeout_seconds: 30
    max_output_bytes: 1048576
    max_input_files: 3
    args:
      - "-input"
      - "{input_file}"
      - "-output"
      - "json"
      - "-detail-limit"
      - "5"
    match:
      file_patterns:
        - "*.jsonl"
      keywords:
        - "influxql"
        - "\"query\""
        - "select"
        - "show series"
        - "show measurements"

  opengemini_storage_analyzer:
    enabled: true
    path: "${work_dir}/bin/tools/opengemini-storage-analyzer"
    timeout_seconds: 30
    max_output_bytes: 1048576
    max_input_files: 10
    args:
      - "--input"
      - "{input_file}"
      - "--format"
      - "json"
    match:
      file_patterns:
        - "*.tssp"
        - "*.tssp.init"
        - "metadata.json"
        - "metaindex.bin"
        - "index.bin"
        - "items.bin"
        - "lens.bin"
        - "*_mergeset.bf"
        - "*_mergeset.bf.last"
        - "*_mergeset.bf.init"
      keywords:
        - "tssp"
        - "mergeset"
        - "metadata.json"
        - "invalid file"
        - "open tssp"

  influxdb_storage_analyzer:
    enabled: true
    path: "${work_dir}/bin/tools/influxdb_storage_analyzer"
    timeout_seconds: 60
    max_output_bytes: 1048576
    max_input_files: 5
    args:
      - "-input"
      - "{input_file}"
      - "-kind"
      - "auto"
      - "-max-samples"
      - "10"
    match:
      file_patterns:
        - "*.tsm"
        - "*.tsi"
      keywords:
        - "_series"
        - "tsm"
        - "tsi"
        - "series file"

llm:
  provider: "stub"
  model: "stub"
  request_timeout_seconds: 120
  max_input_chars: 60000
  max_output_tokens: 4096
EOF

printf '%s\n' "$public_base_url" >"$work_dir/run/server.url"

printf 'Initialized LogAgent work directory: %s\n' "$work_dir"
printf 'Config: %s\n' "$config_path"
