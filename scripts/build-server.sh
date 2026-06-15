#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-logagent-workdir.sh
source "$SCRIPT_DIR/lib-logagent-workdir.sh"

work_dir="$(logagent_require_work_dir)"
repo_root="$(logagent_repo_root)"
logagent_prepare_work_dir "$work_dir"
logagent_require_command cargo

cd "$repo_root"
cargo build --release -p logagent-server

install -m 0755 "$repo_root/target/release/logagent-server" "$(logagent_server_bin "$work_dir")"
"$SCRIPT_DIR/build-tools.sh"

printf 'Installed Server binary: %s\n' "$(logagent_server_bin "$work_dir")"
