#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/smoke-source-built-analyzers.sh [--only <tool>]...

Runs smoke checks for source-built diagnostic analyzers.

Accepted --only values:
  influxql | influxql_analyzer | influxql-analyzer
  flux | flux_query_analyzer | flux-query-analyzer
  opengemini | opengemini_storage_analyzer | opengemini-storage-analyzer
  influxdb | influxdb_storage_analyzer | influxdb-storage-analyzer

Without --only, all four analyzer smoke checks run in this order:
  influxql, flux, opengemini, influxdb
EOF
}

normalize_tool() {
  case "$1" in
    influxql | influxql_analyzer | influxql-analyzer)
      printf 'influxql'
      ;;
    flux | flux_query_analyzer | flux-query-analyzer)
      printf 'flux'
      ;;
    opengemini | opengemini_storage_analyzer | opengemini-storage-analyzer)
      printf 'opengemini'
      ;;
    influxdb | influxdb_storage_analyzer | influxdb-storage-analyzer)
      printf 'influxdb'
      ;;
    *)
      return 1
      ;;
  esac
}

selected=()

while (($# > 0)); do
  case "$1" in
    --only)
      if (($# < 2)); then
        printf 'Missing value for --only\n' >&2
        exit 2
      fi
      if ! tool="$(normalize_tool "$2")"; then
        printf 'Unsupported --only value: %s\n' "$2" >&2
        usage >&2
        exit 2
      fi
      selected+=("$tool")
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if ((${#selected[@]} == 0)); then
  selected=(influxql flux opengemini influxdb)
fi

for tool in "${selected[@]}"; do
  case "$tool" in
    influxql)
      printf 'Running InfluxQL analyzer smoke...\n'
      "$SCRIPT_DIR/smoke-influxql-analyzer.sh"
      ;;
    flux)
      printf 'Running Flux query analyzer smoke...\n'
      "$SCRIPT_DIR/smoke-flux-query-analyzer.sh"
      ;;
    opengemini)
      printf 'Running openGemini storage analyzer smoke...\n'
      "$SCRIPT_DIR/smoke-opengemini-storage-analyzer.sh"
      ;;
    influxdb)
      printf 'Running InfluxDB storage analyzer smoke...\n'
      "$SCRIPT_DIR/smoke-influxdb-storage-analyzer.sh"
      ;;
  esac
done

printf 'Source-built analyzer smoke passed: %s\n' "${selected[*]}"
