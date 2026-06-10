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
SRC_DIR="${LOGAGENT_SRC_DIR:-}"
CONTROL="${LOGAGENT_CONTROL:-$APP_DIR/deploy/logagentctl.sh}"
RUNTIME_BIN="${LOGAGENT_SERVER_BIN:-$APP_DIR/bin/logagent-server}"
WEBUI_DST="$APP_DIR/webui/out"
DATA_DIR="$APP_DIR/data"

export LOGAGENT_APP_DIR="$APP_DIR"

usage() {
  cat <<'USAGE'
Usage: ./rebuild-install.sh [--server-only] [--no-restart]

Environment:
  LOGAGENT_APP_DIR   Runtime directory. Defaults to the parent of deploy/.
  LOGAGENT_SRC_DIR   Source repository directory. Required.

Options:
  --server-only      Build and replace only the Rust server binary.
  --no-restart       Do not restart the server after replacing files.
USAGE
}

server_only=false
restart=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --server-only)
      server_only=true
      shift
      ;;
    --no-restart)
      restart=false
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$SRC_DIR" ]]; then
  echo "LOGAGENT_SRC_DIR is required" >&2
  usage >&2
  exit 1
fi
if [[ ! -f "$SRC_DIR/Cargo.toml" ]]; then
  echo "LOGAGENT_SRC_DIR does not look like the source repository: $SRC_DIR" >&2
  exit 1
fi

BUILD_BIN="$SRC_DIR/target/debug/logagent-server"
WEBUI_SRC="$SRC_DIR/webui/out"

running=false
if "$CONTROL" status >/dev/null 2>&1; then
  running=true
fi

echo "Building Rust server from $SRC_DIR..."
cargo build --manifest-path "$SRC_DIR/Cargo.toml" -p logagent-server

if [[ "$server_only" == false ]]; then
  echo "Building WebUI..."
  npm --prefix "$SRC_DIR/webui" run build
fi

echo "Installing server binary to $RUNTIME_BIN..."
mkdir -p \
  "$(dirname "$RUNTIME_BIN")" \
  "$DATA_DIR/uploads" \
  "$DATA_DIR/sessions" \
  "$DATA_DIR/session_workspaces" \
  "$DATA_DIR/tasks" \
  "$DATA_DIR/workspaces" \
  "$DATA_DIR/cases" \
  "$DATA_DIR/case_imports" \
  "$DATA_DIR/memory"
tmp_bin="$RUNTIME_BIN.tmp.$$"
cp -f "$BUILD_BIN" "$tmp_bin"
chmod +x "$tmp_bin"
mv -f "$tmp_bin" "$RUNTIME_BIN"

if [[ "$server_only" == false ]]; then
  echo "Syncing WebUI static files to $WEBUI_DST..."
  rm -rf "$WEBUI_DST"
  mkdir -p "$APP_DIR/webui"
  cp -a "$WEBUI_SRC" "$APP_DIR/webui/"
fi

if [[ "$restart" == true && "$running" == true ]]; then
  echo "Restarting LogAgent server..."
  "$CONTROL" restart
elif [[ "$restart" == true ]]; then
  echo "Server was not running; install complete."
else
  echo "Install complete; restart skipped."
fi
