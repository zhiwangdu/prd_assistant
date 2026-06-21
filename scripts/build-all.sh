#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

"$SCRIPT_DIR/v2-local.sh" build --with-tools
"$SCRIPT_DIR/auto-deploy-lan.sh"
