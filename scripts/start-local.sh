#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FOREGROUND=false

usage() {
  cat <<'EOF'
Usage: scripts/start-local.sh [--foreground]

  --foreground  Keep the server attached to the current terminal.

Starts the LogAgent server on 127.0.0.1:50992 with examples/server-test.yaml.
Requires LOGAGENT_NATIVE_API_KEY.
EOF
}

while (($# > 0)); do
  case "$1" in
    --foreground)
      FOREGROUND=true
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

require_env() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    printf 'Missing required environment variable: %s\n' "$name" >&2
    exit 1
  fi
}

require_env LOGAGENT_NATIVE_API_KEY

CONFIG="examples/server-test.yaml"
PORT="50992"
PID_FILE="/tmp/logagent-server.pid"
LOG_FILE="/tmp/logagent-server.log"
URL="http://127.0.0.1:${PORT}"

if [[ -f "$PID_FILE" ]]; then
  EXISTING_PID="$(cat "$PID_FILE")"
  if kill -0 "$EXISTING_PID" 2>/dev/null; then
    printf 'LogAgent is already running: pid=%s url=%s\n' "$EXISTING_PID" "$URL"
    exit 0
  fi
  rm -f "$PID_FILE"
fi

cd "$ROOT_DIR"

if [[ ! -f webui/out/index.html ]]; then
  printf 'Building WebUI because webui/out/index.html is missing...\n'
  npm --prefix webui run build
fi

printf 'Building LogAgent Server...\n'
cargo build -p logagent-server

if [[ "$FOREGROUND" == "true" ]]; then
  printf 'Starting LogAgent in foreground: url=%s\n' "$URL"
  printf '%s\n' "$$" >"$PID_FILE"
  exec target/debug/logagent-server --config "$CONFIG"
fi

printf 'Starting LogAgent: url=%s log=%s\n' "$URL" "$LOG_FILE"
nohup target/debug/logagent-server --config "$CONFIG" >"$LOG_FILE" 2>&1 &
SERVER_PID=$!
printf '%s\n' "$SERVER_PID" >"$PID_FILE"
disown "$SERVER_PID" 2>/dev/null || true

for _ in {1..30}; do
  if curl --max-time 1 --silent --fail "$URL/health" >/dev/null; then
    printf 'LogAgent is ready: pid=%s url=%s\n' "$SERVER_PID" "$URL"
    exit 0
  fi
  if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    printf 'LogAgent exited during startup. See %s\n' "$LOG_FILE" >&2
    rm -f "$PID_FILE"
    exit 1
  fi
  sleep 1
done

printf 'LogAgent health check timed out. See %s\n' "$LOG_FILE" >&2
kill "$SERVER_PID" 2>/dev/null || true
rm -f "$PID_FILE"
exit 1
