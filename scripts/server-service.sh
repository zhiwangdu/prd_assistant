#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-logagent-workdir.sh
source "$SCRIPT_DIR/lib-logagent-workdir.sh"

usage() {
  cat <<'EOF'
Usage: scripts/server-service.sh start|stop|restart|status|logs

Environment:
  LOGAGENT_WORK_DIR        Required. Runtime work directory.
  LOGAGENT_NATIVE_API_KEY  Required for start.
  LOGAGENT_SERVER_CONFIG   Optional. Default: $LOGAGENT_WORK_DIR/config/server.yaml
  LOGAGENT_SERVER_URL      Optional. Default: value saved by init-workdir, or http://127.0.0.1:50992
EOF
}

if (($# != 1)); then
  usage >&2
  exit 2
fi

command_name="$1"
work_dir="$(logagent_require_work_dir)"
logagent_prepare_work_dir "$work_dir"

bin_path="$(logagent_server_bin "$work_dir")"
config_path="$(logagent_server_config "$work_dir")"
pid_file="$(logagent_server_pid_file "$work_dir")"
log_file="$(logagent_server_log_file "$work_dir")"
server_url="$(logagent_server_url "$work_dir")"

read_pid() {
  if [[ -f "$pid_file" ]]; then
    head -n 1 "$pid_file"
  fi
}

is_running() {
  local pid="$1"
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

start_server() {
  if [[ -z "${LOGAGENT_NATIVE_API_KEY:-}" ]]; then
    printf 'Missing required environment variable: LOGAGENT_NATIVE_API_KEY\n' >&2
    exit 1
  fi
  if [[ ! -x "$bin_path" ]]; then
    printf 'Missing Server binary: %s\n' "$bin_path" >&2
    printf 'Run scripts/build-server.sh first.\n' >&2
    exit 1
  fi
  if [[ ! -f "$config_path" ]]; then
    printf 'Missing Server config: %s\n' "$config_path" >&2
    printf 'Run scripts/init-workdir.sh first.\n' >&2
    exit 1
  fi
  if [[ ! -f "$work_dir/webui/out/index.html" ]]; then
    printf 'Missing WebUI output: %s\n' "$work_dir/webui/out/index.html" >&2
    printf 'Run scripts/build-webui.sh first.\n' >&2
    exit 1
  fi

  local existing_pid
  existing_pid="$(read_pid || true)"
  if is_running "$existing_pid"; then
    printf 'LogAgent Server is already running: pid=%s url=%s\n' "$existing_pid" "$server_url"
    return 0
  fi
  rm -f "$pid_file"

  logagent_require_command curl

  cd "$work_dir"
  nohup "$bin_path" --config "$config_path" >"$log_file" 2>&1 &
  local server_pid="$!"
  printf '%s\n' "$server_pid" >"$pid_file"
  disown "$server_pid" 2>/dev/null || true

  for _ in {1..30}; do
    if curl --max-time 1 --silent --fail "$server_url/health" >/dev/null; then
      printf 'LogAgent Server is ready: pid=%s url=%s log=%s\n' "$server_pid" "$server_url" "$log_file"
      return 0
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      printf 'LogAgent Server exited during startup. See %s\n' "$log_file" >&2
      rm -f "$pid_file"
      exit 1
    fi
    sleep 1
  done

  printf 'LogAgent Server health check timed out. See %s\n' "$log_file" >&2
  kill "$server_pid" 2>/dev/null || true
  rm -f "$pid_file"
  exit 1
}

stop_server() {
  local pid
  pid="$(read_pid || true)"
  if ! is_running "$pid"; then
    rm -f "$pid_file"
    printf 'LogAgent Server is not running.\n'
    return 0
  fi

  kill "$pid" 2>/dev/null || true
  for _ in {1..30}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      rm -f "$pid_file"
      printf 'Stopped LogAgent Server: pid=%s\n' "$pid"
      return 0
    fi
    sleep 1
  done

  printf 'Server did not stop after SIGTERM, sending SIGKILL: pid=%s\n' "$pid" >&2
  kill -9 "$pid" 2>/dev/null || true
  rm -f "$pid_file"
}

status_server() {
  local pid
  pid="$(read_pid || true)"
  if is_running "$pid"; then
    printf 'LogAgent Server is running: pid=%s url=%s log=%s\n' "$pid" "$server_url" "$log_file"
  else
    printf 'LogAgent Server is stopped.\n'
  fi
}

case "$command_name" in
  start)
    start_server
    ;;
  stop)
    stop_server
    ;;
  restart)
    stop_server
    start_server
    ;;
  status)
    status_server
    ;;
  logs)
    if [[ ! -f "$log_file" ]]; then
      printf 'Missing log file: %s\n' "$log_file" >&2
      exit 1
    fi
    tail -f "$log_file"
    ;;
  -h | --help)
    usage
    ;;
  *)
    printf 'Unknown command: %s\n' "$command_name" >&2
    usage >&2
    exit 2
    ;;
esac
