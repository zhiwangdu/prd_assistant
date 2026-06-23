#!/usr/bin/env bash
# Build openGemini binaries for the dev_selftest docker cluster.
# Runs with cwd = openGemini repo root (the dev_selftest run's source/).
#
# Configurable for intranet via environment variables (inherited from the server
# process by the dev_selftest build child):
#   GOPROXY  Go module proxy (default https://goproxy.cn,direct). Intranet: point
#            at your internal mirror, e.g. GOPROXY=https://goproxy.intranet,direct
#   GOSUMDB  Go checksum DB. Intranet proxies that can't reach sum.golang.org must
#            set GOSUMDB=off (left at Go's default otherwise).
set -euo pipefail

export GOPROXY="${GOPROXY:-https://goproxy.cn,direct}"

set -x
# openGemini ships go 1.24 + older sonic; bump for go 1.26 compatibility.
go mod edit -go=1.26
go get github.com/bytedance/sonic@latest
go mod tidy

mkdir -p build
go build -o build/ts-meta  ./app/ts-meta
go build -o build/ts-store ./app/ts-store
go build -o build/ts-sql   ./app/ts-sql

set +x
echo "=== build output ==="
ls -la build/ts-meta build/ts-store build/ts-sql
