#!/usr/bin/env bash
# ts-meta entrypoint. Substitutes per-node placeholders (OG_ADDR/OG_ID/OG_META_*)
# into the config template, then runs ts-meta.
set -euo pipefail

sed -e "s/{{addr}}/${OG_ADDR}/g" -e "s/{{id}}/${OG_ID}/g" \
    -e "s/{{meta_addr_1}}/${OG_META_1}/g" -e "s/{{meta_addr_2}}/${OG_META_2}/g" -e "s/{{meta_addr_3}}/${OG_META_3}/g" \
    /etc/openGemini/openGemini.conf.template > /etc/openGemini/openGemini.conf

exec /opt/openGemini/build/ts-meta -config /etc/openGemini/openGemini.conf
