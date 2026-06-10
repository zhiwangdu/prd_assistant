#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_APP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="${LOGAGENT_ENV_FILE:-$SCRIPT_DIR/.env}"

if [[ -f "$ENV_FILE" ]]; then
  # shellcheck source=/dev/null
  source "$ENV_FILE"
fi

APP_DIR="${LOGAGENT_APP_DIR:-$DEFAULT_APP_DIR}"
BIN="${LOGAGENT_SERVER_BIN:-$APP_DIR/bin/logagent-server}"
CONFIG="${LOGAGENT_CONFIG:-$APP_DIR/deploy/logagent.yaml}"
PID_FILE="${LOGAGENT_PID_FILE:-$APP_DIR/logagent-server.pid}"
LOG_FILE="${LOGAGENT_LOG_FILE:-$APP_DIR/logagent-server.log}"
HEALTH_URL="${LOGAGENT_HEALTH_URL:-http://127.0.0.1:50992/health}"

export LOGAGENT_APP_DIR="$APP_DIR"

usage() {
  echo "Usage: $0 {start|stop|restart|status|logs}"
}

prepare_runtime_dirs() {
  mkdir -p \
    "$(dirname "$BIN")" \
    "$(dirname "$PID_FILE")" \
    "$(dirname "$LOG_FILE")" \
    "$APP_DIR/data/uploads" \
    "$APP_DIR/data/sessions" \
    "$APP_DIR/data/session_workspaces" \
    "$APP_DIR/data/tasks" \
    "$APP_DIR/data/workspaces" \
    "$APP_DIR/data/cases" \
    "$APP_DIR/data/case_imports" \
    "$APP_DIR/data/memory" \
    "$APP_DIR/webui/out"
}

process_matches_server() {
  local pid="$1"
  local args
  args="$(ps -p "$pid" -o args= 2>/dev/null || true)"
  [[ "$args" == "$BIN --config $CONFIG" ]]
}

find_running_pid() {
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "${pid:-}" ]] && kill -0 "$pid" 2>/dev/null && process_matches_server "$pid"; then
      echo "$pid"
      return 0
    fi
    rm -f "$PID_FILE"
  fi

  local candidate
  for candidate in $(pgrep -f "$(basename "$BIN")" || true); do
    if process_matches_server "$candidate"; then
      echo "$candidate"
      return 0
    fi
  done
}

start_server() {
  local pid
  pid="$(find_running_pid)"
  if [[ -n "${pid:-}" ]]; then
    echo "LogAgent server already running: pid=$pid"
    return 0
  fi

  if [[ ! -x "$BIN" ]]; then
    echo "Server binary not executable: $BIN" >&2
    exit 1
  fi
  if [[ ! -f "$CONFIG" ]]; then
    echo "Config not found: $CONFIG" >&2
    exit 1
  fi

  prepare_runtime_dirs
  cd "$APP_DIR"
  if command -v setsid >/dev/null 2>&1; then
    nohup setsid "$BIN" --config "$CONFIG" >>"$LOG_FILE" 2>&1 < /dev/null &
  else
    nohup "$BIN" --config "$CONFIG" >>"$LOG_FILE" 2>&1 < /dev/null &
  fi
  pid="$!"
  echo "$pid" >"$PID_FILE"
  echo "Started LogAgent server: pid=$pid"
  echo "Config: $CONFIG"
  echo "Log file: $LOG_FILE"
}

stop_server() {
  local pid
  pid="$(find_running_pid)"
  if [[ -z "${pid:-}" ]]; then
    rm -f "$PID_FILE"
    echo "LogAgent server is not running"
    return 0
  fi

  echo "Stopping LogAgent server: pid=$pid"
  kill "$pid" 2>/dev/null || true

  for _ in {1..20}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      rm -f "$PID_FILE"
      echo "Stopped"
      return 0
    fi
    sleep 0.5
  done

  echo "Process did not exit after 10s; sending SIGKILL"
  kill -9 "$pid" 2>/dev/null || true
  rm -f "$PID_FILE"
  echo "Stopped"
}

status_server() {
  local pid
  pid="$(find_running_pid)"
  if [[ -n "${pid:-}" ]]; then
    echo "LogAgent server running: pid=$pid"
    if command -v curl >/dev/null 2>&1; then
      curl -sS "$HEALTH_URL" || true
      echo
    fi
  else
    echo "LogAgent server is not running"
    return 1
  fi
}

case "${1:-}" in
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
    touch "$LOG_FILE"
    tail -n 100 -f "$LOG_FILE"
    ;;
  *)
    usage
    exit 2
    ;;
esac
