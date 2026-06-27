#!/usr/bin/env bash
set -euo pipefail

if [[ ! -x /opt/influxdb/build/influxd ]]; then
  echo "missing executable /opt/influxdb/build/influxd" >&2
  exit 127
fi

mkdir -p /var/lib/influxdb/meta /var/lib/influxdb/data /var/lib/influxdb/wal

exec /opt/influxdb/build/influxd run -config /etc/influxdb/influxdb.conf
