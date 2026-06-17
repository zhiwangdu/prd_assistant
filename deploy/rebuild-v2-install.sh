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
if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env"
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
Usage: ./rebuild-v2-install.sh [--server-only] [--with-tools] [--tools-only] [--only-tool <tool>] [--no-restart]

Environment:
  LOGAGENT_APP_DIR or LOGAGENT_V2_APP_DIR  Runtime directory. Defaults to parent of deploy/.
  LOGAGENT_SRC_DIR                         Source repository directory. Required.

Options:
  --server-only      Install only server-v2 into the runtime virtualenv.
  --with-tools       Build source-referenced analyzer tools into $LOGAGENT_APP_DIR/bin/tools.
  --tools-only       Build analyzer tools only; skip server install, DB init, and WebUI sync.
  --only-tool        Build one analyzer: influxql, flux, opengemini, or influxdb.
  --no-restart       Do not restart V2 after replacing files.
USAGE
}

server_only=false
with_tools=false
tools_only=false
only_tool=""
restart=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --server-only)
      server_only=true
      shift
      ;;
    --with-tools)
      with_tools=true
      shift
      ;;
    --tools-only)
      tools_only=true
      with_tools=true
      server_only=true
      shift
      ;;
    --only-tool)
      if [[ $# -lt 2 ]]; then
        echo "Missing value for --only-tool" >&2
        exit 2
      fi
      only_tool="$2"
      with_tools=true
      shift 2
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
  "$APP_DIR/bin/tools" \
  "$(dirname "$WEBUI_DST")"

if [[ "$tools_only" == false ]]; then
  if [[ ! -x "$VENV_DIR/bin/python" ]]; then
    echo "Creating V2 virtualenv at $VENV_DIR..."
    "$PYTHON_BIN" -m venv "$VENV_DIR"
  fi

  echo "Installing LogAgent V2 server from $SRC_DIR/server-v2..."
  "$VENV_DIR/bin/python" -m pip install --upgrade pip
  "$VENV_DIR/bin/python" -m pip install -e "$SRC_DIR/server-v2"

  echo "Initializing V2 database under $DATA_DIR..."
  "$VENV_DIR/bin/python" -m logagent_v2 init-db
fi

if [[ "$with_tools" == true ]]; then
  echo "Building V2 analyzer tools into $APP_DIR/bin/tools..."
  build_args=(--output-dir "$APP_DIR/bin/tools")
  if [[ -n "$only_tool" ]]; then
    build_args+=(--only "$only_tool")
  fi
  "$SRC_DIR/scripts/build-tools.sh" "${build_args[@]}"
fi

if [[ "$server_only" == false && "$tools_only" == false ]]; then
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
