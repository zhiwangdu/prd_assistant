#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEFAULT_APP_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ENV_FILE="${LOGAGENT_ENV_FILE:-$SCRIPT_DIR/.env}"

if [[ -f "$HOME/.bashrc" ]]; then
  # shellcheck source=/dev/null
  set +u
  set -a
  source "$HOME/.bashrc" || true
  set +a
  set -u
fi
if [[ -f "$ENV_FILE" ]]; then
  # shellcheck source=/dev/null
  set -a
  source "$ENV_FILE"
  set +a
fi

APP_DIR="${LOGAGENT_V2_APP_DIR:-${LOGAGENT_APP_DIR:-$DEFAULT_APP_DIR}}"
VENV_DIR="${LOGAGENT_V2_VENV_DIR:-$APP_DIR/server-v2/.venv}"
PYTHON="${LOGAGENT_V2_PYTHON:-$VENV_DIR/bin/python}"
PID_FILE="${LOGAGENT_V2_PID_FILE:-$APP_DIR/logagent-v2.pid}"
LOG_FILE="${LOGAGENT_V2_LOG_FILE:-$APP_DIR/logagent-v2.log}"
STARTUP_TIMEOUT_SECONDS="${LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS:-30}"

export LOGAGENT_V2_DATA_DIR="${LOGAGENT_V2_DATA_DIR:-$APP_DIR/data-v2}"
export LOGAGENT_V2_WEBUI_DIR="${LOGAGENT_V2_WEBUI_DIR:-$APP_DIR/webui/out}"
export LOGAGENT_V2_HOST="${LOGAGENT_V2_HOST:-127.0.0.1}"
export LOGAGENT_V2_PORT="${LOGAGENT_V2_PORT:-50993}"
export LOGAGENT_V2_API_KEY="${LOGAGENT_V2_API_KEY:-${LOGAGENT_NATIVE_API_KEY:-dev-token}}"

HEALTH_URL="${LOGAGENT_V2_HEALTH_URL:-http://$LOGAGENT_V2_HOST:$LOGAGENT_V2_PORT/health}"

if ! [[ "$STARTUP_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || ((STARTUP_TIMEOUT_SECONDS < 1)); then
  echo "LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS must be a positive integer" >&2
  exit 2
fi

usage() {
  echo "Usage: $0 {start|stop|restart|status|logs}"
}

prepare_runtime_dirs() {
  mkdir -p \
    "$(dirname "$PID_FILE")" \
    "$(dirname "$LOG_FILE")" \
    "$LOGAGENT_V2_DATA_DIR" \
    "$LOGAGENT_V2_WEBUI_DIR"
}

wait_for_health() {
  local pid="$1"
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl not found; skipped health wait for $HEALTH_URL"
    return 0
  fi

  local elapsed
  for ((elapsed = 0; elapsed < STARTUP_TIMEOUT_SECONDS; elapsed++)); do
    if curl --max-time 1 --silent --fail "$HEALTH_URL" >/dev/null; then
      echo "LogAgent V2 server is ready: pid=$pid url=http://$LOGAGENT_V2_HOST:$LOGAGENT_V2_PORT/"
      return 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "LogAgent V2 server exited during startup. See $LOG_FILE" >&2
      rm -f "$PID_FILE"
      return 1
    fi
    sleep 1
  done

  echo "LogAgent V2 health check timed out after ${STARTUP_TIMEOUT_SECONDS}s. See $LOG_FILE" >&2
  kill "$pid" 2>/dev/null || true
  rm -f "$PID_FILE"
  return 1
}

process_matches_server() {
  local pid="$1"
  local args
  args="$(ps -p "$pid" -o args= 2>/dev/null || true)"
  [[ "$args" == *" -m logagent_v2 server"* || "$args" == *"/logagent_v2 "* ]]
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

  if [[ "${LOGAGENT_V2_DISCOVER_PROCESS:-0}" != "1" ]]; then
    return 0
  fi

  local candidate
  for candidate in $(pgrep -f "logagent_v2 server" || true); do
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
    echo "LogAgent V2 server already running: pid=$pid"
    return 0
  fi

  if [[ ! -x "$PYTHON" ]]; then
    echo "V2 Python not executable: $PYTHON" >&2
    echo "Run deploy/rebuild-v2-install.sh first." >&2
    exit 1
  fi

  prepare_runtime_dirs
  "$PYTHON" -m logagent_v2 init-db
  cd "$APP_DIR"
  if command -v setsid >/dev/null 2>&1; then
    nohup setsid "$PYTHON" -m logagent_v2 server >>"$LOG_FILE" 2>&1 < /dev/null &
  else
    nohup "$PYTHON" -m logagent_v2 server >>"$LOG_FILE" 2>&1 < /dev/null &
  fi
  pid="$!"
  echo "$pid" >"$PID_FILE"
  echo "Started LogAgent V2 server: pid=$pid"
  echo "URL: http://$LOGAGENT_V2_HOST:$LOGAGENT_V2_PORT/"
  echo "Data dir: $LOGAGENT_V2_DATA_DIR"
  echo "WebUI dir: $LOGAGENT_V2_WEBUI_DIR"
  echo "Log file: $LOG_FILE"
  wait_for_health "$pid"
}

stop_server() {
  local pid
  pid="$(find_running_pid)"
  if [[ -z "${pid:-}" ]]; then
    rm -f "$PID_FILE"
    echo "LogAgent V2 server is not running"
    return 0
  fi

  echo "Stopping LogAgent V2 server: pid=$pid"
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
    echo "LogAgent V2 server running: pid=$pid"
    if command -v curl >/dev/null 2>&1; then
      curl -sS "$HEALTH_URL" || true
      echo
    fi
  else
    echo "LogAgent V2 server is not running"
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
