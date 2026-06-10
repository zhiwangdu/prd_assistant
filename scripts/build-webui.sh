#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-logagent-workdir.sh
source "$SCRIPT_DIR/lib-logagent-workdir.sh"

work_dir="$(logagent_require_work_dir)"
repo_root="$(logagent_repo_root)"
logagent_prepare_work_dir "$work_dir"
logagent_require_command npm

cd "$repo_root"
npm --prefix webui run build

rm -rf "$work_dir/webui/out"
mkdir -p "$work_dir/webui"
cp -R "$repo_root/webui/out" "$work_dir/webui/out"

printf 'Installed WebUI static output: %s\n' "$work_dir/webui/out"
