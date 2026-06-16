#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

usage() {
  cat <<'EOF'
Usage: scripts/configure-tool-submodules.sh [options]

Configures local Git submodule URLs for LogAgent source-built tools. This
updates .git/config only; .gitmodules remains the committed default. Existing
submodule origin remotes are updated only when the submodule is already an
initialized Git worktree.

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

is_own_git_worktree() {
  local path="$1"
  local top_level
  top_level="$(git -C "$path" rev-parse --show-toplevel 2>/dev/null || true)"
  if [[ -z "$top_level" ]]; then
    return 1
  fi

  [[ "$(cd "$path" && pwd -P)" == "$(cd "$top_level" && pwd -P)" ]]
}

configure_submodule_url() {
  local path="$1"
  local label="$2"
  local url="$3"
  local full_path="$REPO_ROOT/$path"

  if [[ -z "$url" ]]; then
    return
  fi

  git -C "$REPO_ROOT" config "submodule.$path.url" "$url"

  if [[ -d "$full_path" ]] && is_own_git_worktree "$full_path"; then
    git -C "$full_path" remote set-url origin "$url" >/dev/null 2>&1 || true
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
