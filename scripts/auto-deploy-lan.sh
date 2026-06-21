#!/usr/bin/env bash

set -euo pipefail

AUTO_DEPLOY="${LOGAGENT_LAN_AUTO_DEPLOY:-auto}"
if [[ "$AUTO_DEPLOY" == "0" || "$AUTO_DEPLOY" == "false" || "$AUTO_DEPLOY" == "off" ]]; then
  echo "LAN auto deploy skipped: LOGAGENT_LAN_AUTO_DEPLOY=$AUTO_DEPLOY"
  exit 0
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "LAN auto deploy skipped: host is not macOS"
  exit 0
fi

REMOTE_ADDR="${LOGAGENT_LAN_REMOTE_ADDR:-192.168.31.128}"
REMOTE_HOST="${LOGAGENT_LAN_REMOTE_HOST:-duzhiwang@$REMOTE_ADDR}"
REMOTE_DEPLOY_DIR="${LOGAGENT_LAN_REMOTE_DEPLOY_DIR:-/home/duzhiwang/workspace/data/prd_assistant/deploy}"

if ! ping -c 1 -W 1000 "$REMOTE_ADDR" >/dev/null 2>&1; then
  echo "LAN auto deploy skipped: $REMOTE_ADDR is not reachable"
  exit 0
fi

echo "LAN auto deploy: $REMOTE_HOST via $REMOTE_DEPLOY_DIR"
ssh -o BatchMode=yes "$REMOTE_HOST" 'bash -s' -- "$REMOTE_DEPLOY_DIR" <<'REMOTE'
set -euo pipefail

deploy_dir="$1"

if [[ -f "$HOME/.bashrc" ]]; then
  # shellcheck source=/dev/null
  source "$HOME/.bashrc" >/dev/null 2>&1 || true
fi
if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck source=/dev/null
  source "$HOME/.cargo/env" >/dev/null 2>&1 || true
fi
if [[ -f "$deploy_dir/.env" ]]; then
  set -a
  # shellcheck source=/dev/null
  source "$deploy_dir/.env"
  set +a
fi

src_dir="${LOGAGENT_SRC_DIR:-}"
if [[ -z "$src_dir" || ! -d "$src_dir/.git" ]]; then
  echo "LOGAGENT_SRC_DIR is missing or not a git repository: ${src_dir:-<empty>}" >&2
  exit 1
fi

cd "$src_dir"
git pull --ff-only

cd "$deploy_dir"
./rebuild-v2-install.sh
./logagent-v2ctl.sh start
./logagent-v2ctl.sh status
REMOTE
