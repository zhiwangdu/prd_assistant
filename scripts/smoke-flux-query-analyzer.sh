#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

tool="${LOGAGENT_TOOL_FLUX_QUERY_ANALYZER:-$REPO_ROOT/third_party/flux/libflux/target/release/query_stats}"
if [[ ! -x "$tool" ]]; then
  flux_manifest="$REPO_ROOT/third_party/flux/libflux/flux-core/Cargo.toml"
  if [[ ! -f "$flux_manifest" ]]; then
    printf 'Missing Flux submodule at third_party/flux. Run: git submodule update --init third_party/flux\n' >&2
    exit 1
  fi
  cargo build --manifest-path "$flux_manifest" --features query-stats --release --bin query_stats
fi

output="$("$tool" \
  --input <(printf '%s\n' \
    '{"time":"2026-01-01T08:00:00Z","query":"from(bucket:\"prod\") |> range(start:-1h)","duration_ms":45}' \
    '{"time":"2026-01-01T08:01:00Z","query":"invalid query {{{ ","duration_ms":1}') \
  --format json \
  --top-k 2 \
  --max-error-findings 2)"

LOGAGENT_FLUX_SMOKE_JSON="$output" node -e '
const report = JSON.parse(process.env.LOGAGENT_FLUX_SMOKE_JSON);
if (report.tool !== "flux_query_analyzer") throw new Error("unexpected tool id");
if (!String(report.summary || "").includes("rows=2")) throw new Error("missing summary rows");
if (!Array.isArray(report.findings) || report.findings.length === 0) throw new Error("missing findings");
if (!Array.isArray(report.topQueries) || report.topQueries.length !== 1) throw new Error("missing top query");
'

printf 'Flux query analyzer smoke passed.\n'
