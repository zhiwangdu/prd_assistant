#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=lib-logagent-workdir.sh
source "$SCRIPT_DIR/lib-logagent-workdir.sh"

usage() {
  cat <<'EOF'
Usage: scripts/build-tools.sh [--output-dir <dir>] [--only <tool>]

Builds source-referenced diagnostic tools used by LogAgent.

Accepted --only values:
  influxql | influxql_analyzer | influxql-analyzer
  flux | flux_query_analyzer | flux-query-analyzer
  opengemini | opengemini_storage_analyzer | opengemini-storage-analyzer
  influxdb | influxdb_storage_analyzer | influxdb-storage-analyzer

Environment:
  LOGAGENT_TOOLS_BIN_DIR   Optional output directory. Overrides LOGAGENT_WORK_DIR.
  LOGAGENT_WORK_DIR        Optional runtime work directory; output goes to bin/tools/.
  LOGAGENT_GO_CACHE        Optional Go build cache. Defaults to /tmp/logagent-tools-gocache-<go-version>.
  LOGAGENT_SUBMODULE_BASE_URL
                            Optional repository namespace for all tool submodules.
  LOGAGENT_SUBMODULE_INFLUXQL_URL
  LOGAGENT_SUBMODULE_FLUX_URL
  LOGAGENT_SUBMODULE_OPENGEMINI_URL
  LOGAGENT_SUBMODULE_INFLUXDB_URL
                            Optional clone URLs for individual tool submodules.
  LOGAGENT_OPENGEMINI_SRC_DIR
                            Optional openGemini checkout. Defaults to third_party/openGemini
                            or ../openGemini when present.
  LOGAGENT_INFLUXDB_SRC_DIR
                            Optional InfluxDB 1.x checkout with cmd/influxdb_storage_analyzer.
                            Defaults to third_party/influxdb or ../influxdb when present.

Output default:
  target/tools/influxql-analyzer
  target/tools/flux_query_analyzer
  target/tools/opengemini-storage-analyzer
  target/tools/influxdb_storage_analyzer
EOF
}

output_dir="${LOGAGENT_TOOLS_BIN_DIR:-}"
only_tool=""

normalize_only_tool() {
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

while (($# > 0)); do
  case "$1" in
    --output-dir)
      if (($# < 2)); then
        printf 'Missing value for --output-dir\n' >&2
        exit 2
      fi
      output_dir="$2"
      shift 2
      ;;
    --only)
      if (($# < 2)); then
        printf 'Missing value for --only\n' >&2
        exit 2
      fi
      if ! only_tool="$(normalize_only_tool "$2")"; then
        printf 'Unsupported --only value: %s\n' "$2" >&2
        usage >&2
        exit 2
      fi
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

repo_root="$(logagent_repo_root)"
logagent_require_command git
"$SCRIPT_DIR/configure-tool-submodules.sh"

if [[ -z "$output_dir" ]]; then
  if [[ -n "${LOGAGENT_WORK_DIR:-}" ]]; then
    work_dir="$(logagent_require_work_dir)"
    logagent_prepare_work_dir "$work_dir"
    output_dir="$work_dir/bin/tools"
  else
    output_dir="$repo_root/target/tools"
  fi
fi
if [[ "$output_dir" != /* ]]; then
  output_dir="$repo_root/$output_dir"
fi

if [[ -z "$only_tool" || "$only_tool" == "influxql" || "$only_tool" == "opengemini" || "$only_tool" == "influxdb" ]]; then
  logagent_require_command go
fi
if [[ -z "$only_tool" || "$only_tool" == "flux" || "$only_tool" == "influxdb" ]]; then
  logagent_require_command cargo
fi

mkdir -p "$output_dir"
go_cache=""
if [[ -z "$only_tool" || "$only_tool" == "influxql" || "$only_tool" == "opengemini" || "$only_tool" == "influxdb" ]]; then
  go_version="$(go env GOVERSION 2>/dev/null || go version | awk '{print $3}')"
  go_version="${go_version//[^A-Za-z0-9_.-]/_}"
  go_cache="${GOCACHE:-${LOGAGENT_GO_CACHE:-/tmp/logagent-tools-gocache-$go_version}}"
  mkdir -p "$go_cache"
fi

if [[ -z "$only_tool" || "$only_tool" == "influxql" ]]; then
  influxql_dir="$repo_root/third_party/influxql"
  if [[ ! -f "$influxql_dir/go.mod" ]]; then
    git -C "$repo_root" submodule update --init --recursive third_party/influxql
  fi
  if [[ ! -f "$influxql_dir/go.mod" ]]; then
    printf 'Missing InfluxQL analyzer source: %s\n' "$influxql_dir" >&2
    printf 'Run: scripts/configure-tool-submodules.sh if using custom clone URLs, then git submodule update --init --recursive third_party/influxql\n' >&2
    exit 1
  fi

  output_path="$output_dir/influxql-analyzer"
  printf 'Building InfluxQL analyzer: %s\n' "$output_path"
  (
    cd "$influxql_dir"
    GOCACHE="$go_cache" go build -o "$output_path" ./cmd/influxql-analyze
  )
  chmod 0755 "$output_path"

  printf 'Installed InfluxQL analyzer: %s\n' "$output_path"
fi

if [[ -z "$only_tool" || "$only_tool" == "flux" ]]; then
  flux_manifest="$repo_root/third_party/flux/libflux/flux-core/Cargo.toml"
  if [[ ! -f "$flux_manifest" ]]; then
    git -C "$repo_root" submodule update --init --recursive third_party/flux
  fi
  if [[ ! -f "$flux_manifest" ]]; then
    printf 'Missing Flux analyzer source: %s\n' "$repo_root/third_party/flux" >&2
    printf 'Run: scripts/configure-tool-submodules.sh if using custom clone URLs, then git submodule update --init --recursive third_party/flux\n' >&2
    exit 1
  fi

  output_path="$output_dir/flux_query_analyzer"
  printf 'Building Flux query analyzer: %s\n' "$output_path"
  cargo build --manifest-path "$flux_manifest" --features query-stats --release --bin query_stats
  install -m 0755 \
    "$repo_root/third_party/flux/libflux/target/release/query_stats" \
    "$output_path"

  printf 'Installed Flux query analyzer: %s\n' "$output_path"
fi

if [[ -z "$only_tool" || "$only_tool" == "opengemini" ]]; then
  opengemini_dir="${LOGAGENT_OPENGEMINI_SRC_DIR:-}"
  if [[ -z "$opengemini_dir" ]]; then
    if [[ -f "$repo_root/third_party/openGemini/go.mod" ]]; then
      opengemini_dir="$repo_root/third_party/openGemini"
    else
      git -C "$repo_root" submodule update --init --recursive third_party/openGemini
      if [[ -f "$repo_root/third_party/openGemini/go.mod" ]]; then
        opengemini_dir="$repo_root/third_party/openGemini"
      elif [[ -f "$repo_root/../openGemini/go.mod" ]]; then
        opengemini_dir="$repo_root/../openGemini"
      fi
    fi
  fi
  if [[ -n "$opengemini_dir" && -f "$opengemini_dir/go.mod" ]]; then
    output_path="$output_dir/opengemini-storage-analyzer"
    printf 'Building openGemini storage analyzer: %s\n' "$output_path"
    (
      cd "$opengemini_dir"
      GOCACHE="$go_cache" go build -o "$output_path" ./app/opengemini-storage-analyzer
    )
    chmod 0755 "$output_path"
    printf 'Installed openGemini storage analyzer: %s\n' "$output_path"
  elif [[ -n "${LOGAGENT_OPENGEMINI_SRC_DIR:-}" || "$only_tool" == "opengemini" ]]; then
    printf 'Missing openGemini source: %s\n' "${opengemini_dir:-<unset>}" >&2
    exit 1
  else
    printf 'Skipping openGemini storage analyzer; set LOGAGENT_OPENGEMINI_SRC_DIR or initialize third_party/openGemini.\n'
  fi
fi

if [[ -z "$only_tool" || "$only_tool" == "influxdb" ]]; then
  influxdb_dir="${LOGAGENT_INFLUXDB_SRC_DIR:-}"
  if [[ -z "$influxdb_dir" ]]; then
    if [[ -f "$repo_root/third_party/influxdb/go.mod" ]]; then
      influxdb_dir="$repo_root/third_party/influxdb"
    else
      git -C "$repo_root" submodule update --init --recursive third_party/influxdb
      if [[ -f "$repo_root/third_party/influxdb/go.mod" ]]; then
        influxdb_dir="$repo_root/third_party/influxdb"
      elif [[ -f "$repo_root/../influxdb/go.mod" ]]; then
        influxdb_dir="$repo_root/../influxdb"
      fi
    fi
  fi
  if [[ -n "$influxdb_dir" && -f "$influxdb_dir/go.mod" ]]; then
    flux_dir="$repo_root/third_party/flux"
    flux_manifest="$flux_dir/libflux/flux-core/Cargo.toml"
    if [[ ! -f "$flux_manifest" ]]; then
      git -C "$repo_root" submodule update --init --recursive third_party/flux
    fi
    if [[ ! -f "$flux_manifest" ]]; then
      printf 'Missing Flux source required by InfluxDB storage analyzer: %s\n' "$flux_dir" >&2
      printf 'Run: scripts/configure-tool-submodules.sh if using custom clone URLs, then git submodule update --init --recursive third_party/flux\n' >&2
      exit 1
    fi

    output_path="$output_dir/influxdb_storage_analyzer"
    printf 'Building InfluxDB storage analyzer: %s\n' "$output_path"
    (
      cd "$influxdb_dir"
      go_mod_path="$influxdb_dir/go.mod"
      go_sum_path="$influxdb_dir/go.sum"
      go_mod_backup="$(mktemp "${TMPDIR:-/tmp}/logagent-influxdb-go.mod.XXXXXX")"
      go_sum_backup="$(mktemp "${TMPDIR:-/tmp}/logagent-influxdb-go.sum.XXXXXX")"
      had_go_sum=0
      cp "$go_mod_path" "$go_mod_backup"
      if [[ -f "$go_sum_path" ]]; then
        had_go_sum=1
        cp "$go_sum_path" "$go_sum_backup"
      fi
      restore_influxdb_go_files() {
        cp "$go_mod_backup" "$go_mod_path"
        if [[ "$had_go_sum" == "1" ]]; then
          cp "$go_sum_backup" "$go_sum_path"
        else
          rm -f "$go_sum_path"
        fi
        rm -f "$go_mod_backup" "$go_sum_backup"
      }
      trap restore_influxdb_go_files EXIT
      GOFLAGS= go mod edit -replace "github.com/influxdata/flux=$flux_dir"

      tool_go_root="$(go env GOROOT)"
      tool_go_version="$(go env GOVERSION 2>/dev/null || go version | awk '{print $3}')"
      tool_go_version="${tool_go_version//[^A-Za-z0-9_.-]/_}"
      tool_go_cache="${GOCACHE:-${LOGAGENT_GO_CACHE:-/tmp/logagent-tools-gocache-influxdb-$tool_go_version}}"
      mkdir -p "$tool_go_cache"
      export GOROOT="$tool_go_root"
      export PATH="$tool_go_root/bin:$PATH"
      build_env=(GOCACHE="$tool_go_cache" GOSUMDB="${GOSUMDB:-off}")
      if [[ -x "$influxdb_dir/pkg-config.sh" ]]; then
        build_env+=(PKG_CONFIG="$influxdb_dir/pkg-config.sh")
      fi
      env "${build_env[@]}" go build -mod=mod -o "$output_path" ./cmd/influxdb_storage_analyzer
    )
    chmod 0755 "$output_path"
    printf 'Installed InfluxDB storage analyzer: %s\n' "$output_path"
  elif [[ -n "${LOGAGENT_INFLUXDB_SRC_DIR:-}" || "$only_tool" == "influxdb" ]]; then
    printf 'Missing InfluxDB analyzer source: %s\n' "${influxdb_dir:-<unset>}" >&2
    exit 1
  else
    printf 'Skipping InfluxDB storage analyzer; set LOGAGENT_INFLUXDB_SRC_DIR or initialize third_party/influxdb.\n'
  fi
fi
