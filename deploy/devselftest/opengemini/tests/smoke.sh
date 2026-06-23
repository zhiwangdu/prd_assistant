#!/bin/sh
# Dev self-test smoke case for the openGemini cluster.
#
# Connects to the deployed cluster's ts-sql HTTP API (InfluxDB-compatible) and does a
# minimal round-trip: SHOW DATABASES -> CREATE DATABASE -> write a point -> SELECT it back.
# Run by dev_selftest.run_tests inside an ephemeral docker container (default alpine, which
# ships busybox wget) via the executor docker runner. The cluster address is injected by the
# server as DEVSELFTEST_HOST / DEVSELFTEST_PORT; with --network host these resolve to the
# host-exposed ts-sql port (127.0.0.1:8086 by default).
#
# HTTP client: curl if present, else wget (busybox). No apt/network dependency by default.

set -eu

HOST="${DEVSELFTEST_HOST:-127.0.0.1}"
PORT="${DEVSELFTEST_PORT:-8086}"
BASE="http://${HOST}:${PORT}"
DB="smoke_db"

http_get() {
    if command -v curl >/dev/null 2>&1; then
        curl -sf "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O - "$1"
    else
        echo "smoke: neither curl nor wget available" >&2
        return 127
    fi
}

http_post() {
    # $1 = url, $2 = body (may be empty)
    if command -v curl >/dev/null 2>&1; then
        curl -sf -X POST --data "$2" "$1"
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O - --post-data="$2" "$1"
    else
        echo "smoke: neither curl nor wget available" >&2
        return 127
    fi
}

echo "smoke: SHOW DATABASES"
http_get "${BASE}/query?q=SHOW+DATABASES" || true

echo "smoke: CREATE DATABASE ${DB}"
# Tolerate non-zero (database may already exist from a prior run).
http_post "${BASE}/query?q=CREATE+DATABASE+${DB}" "" || true

echo "smoke: write point"
http_post "${BASE}/write?db=${DB}" "smoke,host=t value=1"

echo "smoke: SELECT value FROM smoke"
out=$(http_get "${BASE}/query?db=${DB}&q=SELECT+value+FROM+smoke")
echo "${out}"
# A successful write+read returns a series named "smoke"; an empty result (write failed)
# has no series name.
echo "${out}" | grep -q "smoke" || {
    echo "smoke: SELECT did not return the expected series" >&2
    exit 1
}

echo "smoke: OK"
