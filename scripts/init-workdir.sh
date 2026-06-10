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
