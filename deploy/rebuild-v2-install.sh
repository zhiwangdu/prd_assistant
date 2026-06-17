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
SRC_DIR="${LOGAGENT_SRC_DIR:-}"
CONTROL="${LOGAGENT_V2_CONTROL:-$APP_DIR/deploy/logagent-v2ctl.sh}"
VENV_DIR="${LOGAGENT_V2_VENV_DIR:-$APP_DIR/server-v2/.venv}"
WEBUI_DST="${LOGAGENT_V2_WEBUI_DIR:-$APP_DIR/webui/out}"
DATA_DIR="${LOGAGENT_V2_DATA_DIR:-$APP_DIR/data-v2}"
PYTHON_BIN="${LOGAGENT_V2_BOOTSTRAP_PYTHON:-python3}"

export LOGAGENT_V2_APP_DIR="$APP_DIR"
export LOGAGENT_V2_DATA_DIR="$DATA_DIR"
export LOGAGENT_V2_WEBUI_DIR="$WEBUI_DST"

usage() {
  cat <<'USAGE'
Usage: ./rebuild-v2-install.sh [--server-only] [--no-restart]

Environment:
  LOGAGENT_APP_DIR or LOGAGENT_V2_APP_DIR  Runtime directory. Defaults to parent of deploy/.
  LOGAGENT_SRC_DIR                         Source repository directory. Required.

Options:
  --server-only      Install only server-v2 into the runtime virtualenv.
  --no-restart       Do not restart V2 after replacing files.
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
if [[ ! -f "$SRC_DIR/server-v2/pyproject.toml" ]]; then
  echo "LOGAGENT_SRC_DIR does not contain server-v2/pyproject.toml: $SRC_DIR" >&2
  exit 1
fi

running=false
if "$CONTROL" status >/dev/null 2>&1; then
  running=true
fi

mkdir -p \
  "$APP_DIR/server-v2" \
  "$DATA_DIR" \
  "$(dirname "$WEBUI_DST")"

if [[ ! -x "$VENV_DIR/bin/python" ]]; then
  echo "Creating V2 virtualenv at $VENV_DIR..."
  "$PYTHON_BIN" -m venv "$VENV_DIR"
fi

echo "Installing LogAgent V2 server from $SRC_DIR/server-v2..."
"$VENV_DIR/bin/python" -m pip install --upgrade pip
"$VENV_DIR/bin/python" -m pip install -e "$SRC_DIR/server-v2"

echo "Initializing V2 database under $DATA_DIR..."
"$VENV_DIR/bin/python" -m logagent_v2 init-db

if [[ "$server_only" == false ]]; then
  echo "Building WebUI..."
  npm --prefix "$SRC_DIR/webui" run build

  echo "Syncing WebUI static files to $WEBUI_DST..."
  rm -rf "$WEBUI_DST"
  mkdir -p "$(dirname "$WEBUI_DST")"
  cp -a "$SRC_DIR/webui/out" "$WEBUI_DST"
fi

if [[ "$restart" == true && "$running" == true ]]; then
  echo "Restarting LogAgent V2 server..."
  "$CONTROL" restart
elif [[ "$restart" == true ]]; then
  echo "V2 server was not running; install complete."
else
  echo "V2 install complete; restart skipped."
fi
