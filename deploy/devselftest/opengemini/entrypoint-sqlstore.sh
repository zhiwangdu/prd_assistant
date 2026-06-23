#!/usr/bin/env bash
# ts-store + ts-sql entrypoint. openGemini requires meta -> store -> sql startup
# order; depends_on only orders container start, so this script gates on meta
# reachability before starting store, then gates on store before starting sql.
# ts-store binds its ingest port on the container's own IP (store-ingest-addr),
# so the store readiness check uses OG_ADDR (not 127.0.0.1).
set -euo pipefail

sed -e "s/{{addr}}/${OG_ADDR}/g" -e "s/{{id}}/${OG_ID}/g" \
    -e "s/{{meta_addr_1}}/${OG_META_1}/g" -e "s/{{meta_addr_2}}/${OG_META_2}/g" -e "s/{{meta_addr_3}}/${OG_META_3}/g" \
    /etc/openGemini/openGemini.conf.template > /etc/openGemini/openGemini.conf

wait_tcp() {
  local host="$1" port="$2" name="$3"
  echo "waiting for ${name} (${host}:${port}) ..."
  for _ in $(seq 1 180); do
    if timeout 1 bash -c "echo > /dev/tcp/${host}/${port}" 2>/dev/null; then
      echo "${name} reachable"
      return 0
    fi
    sleep 1
  done
  echo "timeout waiting for ${name} (${host}:${port})" >&2
  return 1
}

# 1. Wait for all 3 ts-meta http (8091) to be reachable.
for h in "$OG_META_1" "$OG_META_2" "$OG_META_3"; do
  wait_tcp "$h" 8091 "meta ${h}" || exit 1
done

# 2. Start ts-store in the background.
echo "starting ts-store"
/opt/openGemini/build/ts-store -config /etc/openGemini/openGemini.conf &

# 3. Wait for ts-store ingest (8400) on this container's own IP.
wait_tcp "$OG_ADDR" 8400 "ts-store" || exit 1

# 4. Start ts-sql in the foreground (keeps the container alive).
echo "starting ts-sql"
exec /opt/openGemini/build/ts-sql -config /etc/openGemini/openGemini.conf
