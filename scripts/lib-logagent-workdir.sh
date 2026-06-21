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
    "$work_dir/bin/tools" \
    "$work_dir/webui"
}

logagent_require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$name" >&2
    exit 1
  fi
}
