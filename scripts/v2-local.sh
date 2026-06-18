#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/v2-local.sh build|start|stop|restart|status|logs|smoke-tools [options]

Commands:
  build       Create/update the V2 virtualenv, install server-v2, and optionally build WebUI/tools.
  start       Start the local V2 server, creating the virtualenv when missing.
  stop        Stop the local V2 server.
  restart     Stop and start the local V2 server.
  status      Show pid and health status.
  logs        Tail the local V2 log file.
  smoke-tools Run source-built analyzer smoke checks.

Options:
  --foreground       With start/restart, run attached to the current terminal.
  --build-webui      Force WebUI build before build/start.
  --no-build         With start/restart, skip editable install unless the venv is missing.
  --with-tools       Build source-referenced analyzers into LOGAGENT_V2_TOOLS_DIR.
  --only-tool <name> Build or smoke one analyzer by short name or V2 toolId.
                     Accepted: influxql/influxql_analyzer,
                     flux/flux_query_analyzer,
                     opengemini/opengemini_storage_analyzer,
                     influxdb/influxdb_storage_analyzer.

Environment:
  LOGAGENT_V2_API_KEY       Defaults to LOGAGENT_NATIVE_API_KEY or dev-token.
  LOGAGENT_V2_DATA_DIR      Defaults to /tmp/logagent-v2-local.
  LOGAGENT_V2_PORT          Defaults to 50993.
  LOGAGENT_V2_TOOLS_DIR     Defaults to <repo>/target/tools.
EOF
}

if (($# < 1)); then
  usage >&2
  exit 2
fi

command_name="$1"
shift

foreground=false
build_webui=false
skip_build=false
with_tools=false
only_tool=""

normalize_only_tool() {
  case "$1" in
    influxql | influxql_analyzer | influxql-analyzer)
      printf 'influxql'
      ;;
    flux | flux_query_analyzer | flux-query-analyzer)
      printf 'flux'
      ;;
    opengemini | opengemini_storage_analyzer | opengemini-storage-analyzer)
      printf 'opengemini'
      ;;
    influxdb | influxdb_storage_analyzer | influxdb-storage-analyzer)
      printf 'influxdb'
      ;;
    *)
      return 1
      ;;
  esac
}

while (($# > 0)); do
  case "$1" in
    --foreground)
      foreground=true
      shift
      ;;
    --build-webui)
      build_webui=true
      shift
      ;;
    --no-build)
      skip_build=true
      shift
      ;;
    --with-tools)
      with_tools=true
      shift
      ;;
    --only-tool)
      if (($# < 2)); then
        printf 'Missing value for --only-tool\n' >&2
        exit 2
      fi
      if ! only_tool="$(normalize_only_tool "$2")"; then
        printf 'Unsupported --only-tool value: %s\n' "$2" >&2
        usage >&2
        exit 2
      fi
      with_tools=true
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

export LOGAGENT_V2_APP_DIR="${LOGAGENT_V2_APP_DIR:-$ROOT_DIR}"
export LOGAGENT_V2_DATA_DIR="${LOGAGENT_V2_DATA_DIR:-/tmp/logagent-v2-local}"
export LOGAGENT_V2_WEBUI_DIR="${LOGAGENT_V2_WEBUI_DIR:-$ROOT_DIR/webui/out}"
export LOGAGENT_V2_HOST="${LOGAGENT_V2_HOST:-127.0.0.1}"
export LOGAGENT_V2_PORT="${LOGAGENT_V2_PORT:-50993}"
export LOGAGENT_V2_API_KEY="${LOGAGENT_V2_API_KEY:-${LOGAGENT_NATIVE_API_KEY:-dev-token}}"
export LOGAGENT_V2_TOOLS_DIR="${LOGAGENT_V2_TOOLS_DIR:-$ROOT_DIR/target/tools}"

VENV_DIR="${LOGAGENT_V2_VENV_DIR:-$ROOT_DIR/server-v2/.venv}"
PYTHON_BIN="${LOGAGENT_V2_BOOTSTRAP_PYTHON:-python3}"
PYTHON="${LOGAGENT_V2_PYTHON:-$VENV_DIR/bin/python}"
PID_FILE="${LOGAGENT_V2_PID_FILE:-/tmp/logagent-v2-local.pid}"
LOG_FILE="${LOGAGENT_V2_LOG_FILE:-/tmp/logagent-v2-local.log}"
HEALTH_URL="${LOGAGENT_V2_HEALTH_URL:-http://$LOGAGENT_V2_HOST:$LOGAGENT_V2_PORT/health}"
TOOLS_URL="${LOGAGENT_V2_TOOLS_URL:-http://$LOGAGENT_V2_HOST:$LOGAGENT_V2_PORT/api/v2/tools}"
STARTUP_TIMEOUT_SECONDS="${LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS:-30}"

if ! [[ "$STARTUP_TIMEOUT_SECONDS" =~ ^[0-9]+$ ]] || ((STARTUP_TIMEOUT_SECONDS < 1)); then
  printf 'LOGAGENT_V2_STARTUP_TIMEOUT_SECONDS must be a positive integer\n' >&2
  exit 2
fi

require_command() {
  local name="$1"
  if ! command -v "$name" >/dev/null 2>&1; then
    printf 'Missing required command: %s\n' "$name" >&2
    exit 1
  fi
}

read_pid() {
  if [[ -f "$PID_FILE" ]]; then
    head -n 1 "$PID_FILE"
  fi
}

is_running() {
  local pid="$1"
  [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null
}

python_for_json() {
  if [[ -x "$PYTHON" ]]; then
    printf '%s\n' "$PYTHON"
    return 0
  fi
  command -v python3 2>/dev/null || true
}

print_source_built_analyzers_status() {
  command -v curl >/dev/null 2>&1 || return 0

  local catalog
  if ! catalog="$(curl --max-time 2 --silent --fail \
    -H "Authorization: Bearer $LOGAGENT_V2_API_KEY" \
    "$TOOLS_URL" 2>/dev/null)"; then
    printf 'Analyzer tools: unavailable (tools API request failed)\n'
    return 0
  fi

  local parser
  parser="$(python_for_json)"
  if [[ -z "$parser" ]]; then
    printf 'Analyzer tools: unavailable (python3 not found for catalog parsing)\n'
    return 0
  fi

  if ! printf '%s' "$catalog" | "$parser" -c '
import json
import sys

doc = json.load(sys.stdin)
items = doc.get("sourceBuiltAnalyzers") or []
if not items:
    print("Analyzer tools: no source-built analyzer summary")
    raise SystemExit(0)

print("Analyzer tools:")
for item in items:
    print(
        "  - {tool_id}: status={status}, enabled={enabled}, runnable={runnable}".format(
            tool_id=item.get("toolId", "<unknown>"),
            status=item.get("status", "<unknown>"),
            enabled=str(bool(item.get("enabled"))).lower(),
            runnable=str(bool(item.get("runnable"))).lower(),
        )
    )
'; then
    printf 'Analyzer tools: unavailable (failed to parse tools catalog)\n'
  fi
}

build_v2() {
  require_command "$PYTHON_BIN"
  if [[ ! -x "$PYTHON" ]]; then
    printf 'Creating V2 virtualenv: %s\n' "$VENV_DIR"
    "$PYTHON_BIN" -m venv "$VENV_DIR"
  fi

  printf 'Installing server-v2 in editable mode...\n'
  "$PYTHON" -m pip install --upgrade pip
  "$PYTHON" -m pip install -e "$ROOT_DIR/server-v2"

  if [[ "$build_webui" == true || ! -f "$LOGAGENT_V2_WEBUI_DIR/index.html" ]]; then
    require_command npm
    printf 'Building WebUI: %s\n' "$LOGAGENT_V2_WEBUI_DIR"
    npm --prefix "$ROOT_DIR/webui" run build
  fi

  if [[ "$with_tools" == true ]]; then
    printf 'Building V2 analyzer tools: %s\n' "$LOGAGENT_V2_TOOLS_DIR"
    tool_args=(--output-dir "$LOGAGENT_V2_TOOLS_DIR")
    if [[ -n "$only_tool" ]]; then
      tool_args+=(--only "$only_tool")
    fi
    "$ROOT_DIR/scripts/build-tools.sh" "${tool_args[@]}"
  fi
}

ensure_runtime() {
  mkdir -p \
    "$LOGAGENT_V2_DATA_DIR" \
    "$LOGAGENT_V2_TOOLS_DIR" \
    "$(dirname "$PID_FILE")" \
    "$(dirname "$LOG_FILE")"

  if [[ ! -x "$PYTHON" ]]; then
    build_v2
  elif [[ "$skip_build" == false && ( "$build_webui" == true || "$with_tools" == true || ! -f "$LOGAGENT_V2_WEBUI_DIR/index.html" ) ]]; then
    build_v2
  fi
  if [[ ! -x "$PYTHON" ]]; then
    printf 'Missing V2 Python executable: %s\n' "$PYTHON" >&2
    exit 1
  fi
  "$PYTHON" -m logagent_v2 init-db
}

wait_for_health() {
  local pid="$1"
  require_command curl
  for ((elapsed = 0; elapsed < STARTUP_TIMEOUT_SECONDS; elapsed++)); do
    if curl --max-time 1 --silent --fail "$HEALTH_URL" >/dev/null; then
      printf 'LogAgent V2 is ready: pid=%s url=http://%s:%s/\n' "$pid" "$LOGAGENT_V2_HOST" "$LOGAGENT_V2_PORT"
      return 0
    fi
    if ! kill -0 "$pid" 2>/dev/null; then
      printf 'LogAgent V2 exited during startup. See %s\n' "$LOG_FILE" >&2
      rm -f "$PID_FILE"
      return 1
    fi
    sleep 1
  done

  printf 'LogAgent V2 health check timed out. See %s\n' "$LOG_FILE" >&2
  kill "$pid" 2>/dev/null || true
  rm -f "$PID_FILE"
  return 1
}

start_v2() {
  local existing_pid
  existing_pid="$(read_pid || true)"
  if is_running "$existing_pid"; then
    printf 'LogAgent V2 is already running: pid=%s url=http://%s:%s/\n' "$existing_pid" "$LOGAGENT_V2_HOST" "$LOGAGENT_V2_PORT"
    return 0
  fi
  rm -f "$PID_FILE"

  ensure_runtime

  if [[ "$foreground" == true ]]; then
    printf 'Starting LogAgent V2 in foreground: url=http://%s:%s/\n' "$LOGAGENT_V2_HOST" "$LOGAGENT_V2_PORT"
    exec "$PYTHON" -m logagent_v2 server
  fi

  printf 'Starting LogAgent V2: url=http://%s:%s/ log=%s\n' "$LOGAGENT_V2_HOST" "$LOGAGENT_V2_PORT" "$LOG_FILE"
  cd "$ROOT_DIR"
  nohup "$PYTHON" -m logagent_v2 server >"$LOG_FILE" 2>&1 &
  local server_pid="$!"
  printf '%s\n' "$server_pid" >"$PID_FILE"
  disown "$server_pid" 2>/dev/null || true
  wait_for_health "$server_pid"
}

stop_v2() {
  local pid
  pid="$(read_pid || true)"
  if ! is_running "$pid"; then
    rm -f "$PID_FILE"
    printf 'LogAgent V2 is not running.\n'
    return 0
  fi

  printf 'Stopping LogAgent V2: pid=%s\n' "$pid"
  kill "$pid" 2>/dev/null || true
  for _ in {1..30}; do
    if ! kill -0 "$pid" 2>/dev/null; then
      rm -f "$PID_FILE"
      printf 'Stopped LogAgent V2: pid=%s\n' "$pid"
      return 0
    fi
    sleep 1
  done

  printf 'LogAgent V2 did not stop after SIGTERM, sending SIGKILL: pid=%s\n' "$pid" >&2
  kill -9 "$pid" 2>/dev/null || true
  rm -f "$PID_FILE"
}

status_v2() {
  local pid
  pid="$(read_pid || true)"
  if is_running "$pid"; then
    printf 'LogAgent V2 is running: pid=%s url=http://%s:%s/ log=%s\n' "$pid" "$LOGAGENT_V2_HOST" "$LOGAGENT_V2_PORT" "$LOG_FILE"
    if command -v curl >/dev/null 2>&1; then
      curl -sS "$HEALTH_URL" || true
      printf '\n'
    fi
    print_source_built_analyzers_status
  else
    printf 'LogAgent V2 is stopped.\n'
    return 1
  fi
}

smoke_tools() {
  local smoke_args=()
  if [[ -n "$only_tool" ]]; then
    smoke_args+=(--only "$only_tool")
  fi
  "$ROOT_DIR/scripts/smoke-source-built-analyzers.sh" "${smoke_args[@]}"
}

case "$command_name" in
  build)
    mkdir -p "$LOGAGENT_V2_DATA_DIR" "$LOGAGENT_V2_TOOLS_DIR"
    build_v2
    "$PYTHON" -m logagent_v2 init-db
    ;;
  start)
    start_v2
    ;;
  stop)
    stop_v2
    ;;
  restart)
    stop_v2
    start_v2
    ;;
  status)
    status_v2
    ;;
  logs)
    touch "$LOG_FILE"
    tail -n 100 -f "$LOG_FILE"
    ;;
  smoke-tools)
    smoke_tools
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
