#!/usr/bin/env bash

logagent_repo_root() {
  cd "$(dirname "${BASH_SOURCE[0]}")/.." >/dev/null 2>&1
  pwd -P
}

logagent_require_work_dir() {
  if [[ -z "${LOGAGENT_WORK_DIR:-}" ]]; then
    printf 'Missing required environment variable: LOGAGENT_WORK_DIR\n' >&2
    exit 1
  fi

  mkdir -p "$LOGAGENT_WORK_DIR"
  cd "$LOGAGENT_WORK_DIR" >/dev/null 2>&1
  pwd -P
}

logagent_prepare_work_dir() {
  local work_dir="$1"
  mkdir -p \
    "$work_dir/bin" \
    "$work_dir/config" \
    "$work_dir/data" \
    "$work_dir/logs" \
    "$work_dir/run" \
    "$work_dir/webui"
}

logagent_server_bin() {
  printf '%s/bin/logagent-server\n' "$1"
}

logagent_server_config() {
  if [[ -n "${LOGAGENT_SERVER_CONFIG:-}" ]]; then
    printf '%s\n' "$LOGAGENT_SERVER_CONFIG"
  else
    printf '%s/config/server.yaml\n' "$1"
  fi
}

logagent_server_pid_file() {
  printf '%s/run/logagent-server.pid\n' "$1"
}

logagent_server_log_file() {
  printf '%s/logs/logagent-server.log\n' "$1"
}

logagent_server_url() {
  local work_dir="$1"
  if [[ -n "${LOGAGENT_SERVER_URL:-}" ]]; then
    printf '%s\n' "$LOGAGENT_SERVER_URL"
  elif [[ -f "$work_dir/run/server.url" ]]; then
    head -n 1 "$work_dir/run/server.url"
  else
    printf 'http://127.0.0.1:50992\n'
  fi
}

logagent_require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$name" >&2
    exit 1
  fi
}
