#!/bin/sh
# Dev self-test smoke case for the InfluxDB OSS single-node server.
#
# The deployed target exposes the InfluxDB v1 HTTP API. This script performs a
# minimal round-trip: SHOW DATABASES -> CREATE DATABASE -> write a point -> SELECT
# it back. It runs in an ephemeral Docker test container; the Server injects
# DEVSELFTEST_HOST / DEVSELFTEST_PORT.
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
http_get "${BASE}/query?q=SHOW+DATABASES"

echo "smoke: CREATE DATABASE ${DB}"
http_post "${BASE}/query?q=CREATE+DATABASE+${DB}" "" || true

echo "smoke: write point"
http_post "${BASE}/write?db=${DB}" "smoke,host=t value=1"

echo "smoke: SELECT value FROM smoke"
deadline=$(( $(date +%s) + 20 ))
while :; do
    out=$(http_get "${BASE}/query?db=${DB}&q=SELECT+value+FROM+smoke" || true)
    echo "${out}"
    if echo "${out}" | grep -q "smoke"; then
        break
    fi
    if [ "$(date +%s)" -ge "${deadline}" ]; then
        echo "smoke: SELECT did not return the expected series" >&2
        exit 1
    fi
    sleep 1
done

echo "smoke: OK"
