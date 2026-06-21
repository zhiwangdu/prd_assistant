#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-logagent-workdir.sh
source "$SCRIPT_DIR/lib-logagent-workdir.sh"

usage() {
  cat <<'EOF'
Usage: scripts/build-webui.sh [--output-dir <dir>]

Builds the WebUI static bundle. When --output-dir or LOGAGENT_WEBUI_OUT_DIR is
set, the built webui/out directory is copied there. LOGAGENT_WORK_DIR remains
accepted for compatibility and copies to $LOGAGENT_WORK_DIR/webui/out.
EOF
}

copy_to="${LOGAGENT_WEBUI_OUT_DIR:-}"

while (($# > 0)); do
  case "$1" in
    --output-dir)
      if (($# < 2)); then
        printf 'Missing value for --output-dir\n' >&2
        exit 2
      fi
      copy_to="$2"
      shift 2
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
done

repo_root="$(logagent_repo_root)"
logagent_require_command npm

cd "$repo_root"
npm --prefix webui run build

if [[ -z "$copy_to" && -n "${LOGAGENT_WORK_DIR:-}" ]]; then
  work_dir="$(logagent_require_work_dir)"
  logagent_prepare_work_dir "$work_dir"
  copy_to="$work_dir/webui/out"
fi

if [[ -n "$copy_to" ]]; then
  if [[ "$copy_to" != /* ]]; then
    copy_to="$repo_root/$copy_to"
  fi
  rm -rf "$copy_to"
  mkdir -p "$(dirname "$copy_to")"
  cp -R "$repo_root/webui/out" "$copy_to"
  printf 'Installed WebUI static output: %s\n' "$copy_to"
else
  printf 'Built WebUI static output: %s\n' "$repo_root/webui/out"
fi
