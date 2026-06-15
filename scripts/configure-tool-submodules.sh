#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/configure-tool-submodules.sh [options]

Configures local Git submodule URLs for LogAgent source-built tools. This
updates .git/config only; .gitmodules remains the committed default.

Options:
  --base-url <url>          Repository namespace containing influxql.git,
                            flux.git, openGemini.git, and influxdb.git.
  --influxql-url <url>      Clone URL for third_party/influxql.
  --flux-url <url>          Clone URL for third_party/flux.
  --opengemini-url <url>    Clone URL for third_party/openGemini.
  --influxdb-url <url>      Clone URL for third_party/influxdb.
  -h, --help                Show this help.

Environment:
  LOGAGENT_SUBMODULE_BASE_URL
  LOGAGENT_SUBMODULE_INFLUXQL_URL
  LOGAGENT_SUBMODULE_FLUX_URL
  LOGAGENT_SUBMODULE_OPENGEMINI_URL
  LOGAGENT_SUBMODULE_INFLUXDB_URL

Example:
  export LOGAGENT_SUBMODULE_BASE_URL="ssh://git@gitlab.internal/zhiwangdu"
  scripts/configure-tool-submodules.sh
  git submodule update --init --recursive third_party/flux
EOF
}

base_url="${LOGAGENT_SUBMODULE_BASE_URL:-}"
influxql_url="${LOGAGENT_SUBMODULE_INFLUXQL_URL:-}"
flux_url="${LOGAGENT_SUBMODULE_FLUX_URL:-}"
opengemini_url="${LOGAGENT_SUBMODULE_OPENGEMINI_URL:-}"
influxdb_url="${LOGAGENT_SUBMODULE_INFLUXDB_URL:-}"

while (($# > 0)); do
  case "$1" in
    --base-url)
      if (($# < 2)); then
        printf 'Missing value for --base-url\n' >&2
        exit 2
      fi
      base_url="$2"
      shift 2
      ;;
    --influxql-url)
      if (($# < 2)); then
        printf 'Missing value for --influxql-url\n' >&2
        exit 2
      fi
      influxql_url="$2"
      shift 2
      ;;
    --flux-url)
      if (($# < 2)); then
        printf 'Missing value for --flux-url\n' >&2
        exit 2
      fi
      flux_url="$2"
      shift 2
      ;;
    --opengemini-url)
      if (($# < 2)); then
        printf 'Missing value for --opengemini-url\n' >&2
        exit 2
      fi
      opengemini_url="$2"
      shift 2
      ;;
    --influxdb-url)
      if (($# < 2)); then
        printf 'Missing value for --influxdb-url\n' >&2
        exit 2
      fi
      influxdb_url="$2"
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

url_from_base() {
  local explicit_url="$1"
  local repo_name="$2"
  if [[ -n "$explicit_url" ]]; then
    printf '%s\n' "$explicit_url"
  elif [[ -n "$base_url" ]]; then
    printf '%s/%s.git\n' "${base_url%/}" "$repo_name"
  fi
}

configure_submodule_url() {
  local path="$1"
  local label="$2"
  local url="$3"

  if [[ -z "$url" ]]; then
    return
  fi

  git -C "$REPO_ROOT" config "submodule.$path.url" "$url"

  if [[ -d "$REPO_ROOT/$path" ]] && git -C "$REPO_ROOT/$path" rev-parse --git-dir >/dev/null 2>&1; then
    git -C "$REPO_ROOT/$path" remote set-url origin "$url" >/dev/null 2>&1 || true
  fi

  printf 'Configured %s submodule URL: %s\n' "$label" "$url"
}

configure_submodule_url \
  "third_party/influxql" \
  "InfluxQL" \
  "$(url_from_base "$influxql_url" "influxql")"
configure_submodule_url \
  "third_party/flux" \
  "Flux" \
  "$(url_from_base "$flux_url" "flux")"
configure_submodule_url \
  "third_party/openGemini" \
  "openGemini" \
  "$(url_from_base "$opengemini_url" "openGemini")"
configure_submodule_url \
  "third_party/influxdb" \
  "InfluxDB" \
  "$(url_from_base "$influxdb_url" "influxdb")"
