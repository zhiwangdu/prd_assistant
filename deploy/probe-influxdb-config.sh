#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BASHRC_FILE="${LOGAGENT_BASHRC_FILE:-$HOME/.bashrc}"

DEFAULT_REPO_URL="ssh://git@github.com/zhiwangdu/influxdb.git"
DEFAULT_GIT_REF="master-1.x"

APP_DIR=""
SRC_DIR=""
OUTPUT=""
DATA_DIR=""
REPO_URL="${LOGAGENT_INFLUXDB_REPO_URL:-$DEFAULT_REPO_URL}"
GIT_REF="${LOGAGENT_INFLUXDB_REF:-$DEFAULT_GIT_REF}"
BIND="${LOGAGENT_BIND:-127.0.0.1:50995}"
PUBLIC_BASE_URL="${LOGAGENT_PUBLIC_BASE_URL:-http://127.0.0.1:50995}"
API_KEY_ENV="${LOGAGENT_API_KEY_ENV:-LOGAGENT_NATIVE_API_KEY}"
SERVER_PORT="50995"
DB_PORT="${INFLUXDB_PORT:-8086}"
BUILDER_IMAGE="${INFLUXDB_BUILDER_IMAGE:-golang:1.26-bookworm}"
BASE_IMAGE="${INFLUXDB_BASE_IMAGE:-ubuntu:24.04}"
TEST_IMAGE="${DEVSELFTEST_TEST_IMAGE:-alpine:3.20}"
PRINT_CONFIG=false
FORCE=false
STRICT_GIT=true

usage() {
  cat <<'USAGE'
Usage:
  deploy/probe-influxdb-config.sh [options]

Probes the local environment and writes an InfluxDB OSS v1 dev_selftest Server
config. The generated pipeline builds only the single-node influxd binary from
git@github.com:zhiwangdu/influxdb.git (normalized to ssh:// form) on branch
master-1.x, deploys one local Docker container, and runs an HTTP smoke test.

Defaults:
  output        $LOGAGENT_APP_DIR/deploy/server-influxdb.yaml
  data dir      $LOGAGENT_APP_DIR/data
  repo          ssh://git@github.com/zhiwangdu/influxdb.git
  ref           master-1.x
  builder image golang:1.26-bookworm
  runtime image ubuntu:24.04
  test image    alpine:3.20

Options:
  --app-dir DIR            Runtime app dir. Defaults to LOGAGENT_APP_DIR.
  --src-dir DIR            prd_assistant repo dir. Defaults to LOGAGENT_SRC_DIR or this checkout.
  --output FILE            Config output path.
  --data-dir DIR           storage.data_dir value.
  --repo-url URL           Allowed InfluxDB git repo URL. Use ssh:// form for Server config.
  --git-ref REF            Allowed InfluxDB git ref. Default: master-1.x.
  --bind ADDR:PORT         Server bind address. Default: 127.0.0.1:50995.
  --public-base-url URL    Server public_base_url.
  --api-key-env NAME       Auth env var name. Default: LOGAGENT_NATIVE_API_KEY.
  --db-port PORT           Host port for InfluxDB 8086. Default: 8086.
  --builder-image IMAGE    Docker build image. Default: golang:1.26-bookworm.
  --base-image IMAGE       Runtime image. Default: ubuntu:24.04.
  --test-image IMAGE       Test runner image. Default: alpine:3.20.
  --print                  Print generated config after writing it.
  --force                  Write config even if required probes fail.
  --skip-git-probe         Do not run git ls-remote for repo/ref reachability.
  -h, --help               Show this help.

Environment overrides:
  LOGAGENT_INFLUXDB_REPO_URL, LOGAGENT_INFLUXDB_REF, INFLUXDB_PORT,
  INFLUXDB_BUILDER_IMAGE, INFLUXDB_BASE_IMAGE, DEVSELFTEST_TEST_IMAGE,
  INFLUXDB_RUST_TOOLCHAIN, GOPROXY, GOSUMDB, LOGAGENT_BIND,
  LOGAGENT_PUBLIC_BASE_URL, LOGAGENT_API_KEY_ENV, LOGAGENT_BASHRC_FILE.
USAGE
}

read_export_value() {
  local name="$1"
  local file="$2"
  [[ -f "$file" ]] || return 1
  awk -v key="$name" '
    $0 ~ "^[[:space:]]*(export[[:space:]]+)?" key "=" {
      line = $0
      sub(/^[[:space:]]*export[[:space:]]+/, "", line)
      sub("^[[:space:]]*" key "=", "", line)
      sub(/[[:space:]]*#.*$/, "", line)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
      if ((line ~ /^".*"$/) || (line ~ /^'\''.*'\''$/)) {
        line = substr(line, 2, length(line) - 2)
      }
      value = line
    }
    END {
      if (value != "") {
        print value
      } else {
        exit 1
      }
    }
  ' "$file"
}

expand_home() {
  local value="$1"
  case "$value" in
    "~") printf '%s\n' "$HOME" ;;
    "~/"*) printf '%s/%s\n' "$HOME" "${value#~/}" ;;
    *) printf '%s\n' "$value" ;;
  esac
}

abs_dir() {
  local dir="$1"
  dir="$(expand_home "$dir")"
  (cd "$dir" && pwd)
}

yaml_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  printf '%s' "$value"
}

port_from_bind() {
  local bind_value="$1"
  printf '%s\n' "${bind_value##*:}"
}

port_listening() {
  local port="$1"
  if command -v lsof >/dev/null 2>&1; then
    lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1
    return $?
  fi
  if command -v ss >/dev/null 2>&1; then
    ss -ltn "( sport = :$port )" 2>/dev/null | awk 'NR > 1 { found = 1 } END { exit found ? 0 : 1 }'
    return $?
  fi
  return 1
}

docker_image_available() {
  local image="$1"
  local image_id=""
  [[ -n "$DOCKER_BIN" ]] || return 1
  if "$DOCKER_BIN" image inspect "$image" >/dev/null 2>&1; then
    return 0
  fi
  image_id="$("$DOCKER_BIN" images -q "$image" 2>/dev/null | head -n 1 || true)"
  if [[ -n "$image_id" ]] && "$DOCKER_BIN" image inspect "$image_id" >/dev/null 2>&1; then
    return 0
  fi
  return 1
}

errors=()
warnings=()

add_error() {
  errors+=("$1")
}

add_warning() {
  warnings+=("$1")
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app-dir) APP_DIR="${2:-}"; shift 2 ;;
    --src-dir) SRC_DIR="${2:-}"; shift 2 ;;
    --output) OUTPUT="${2:-}"; shift 2 ;;
    --data-dir) DATA_DIR="${2:-}"; shift 2 ;;
    --repo-url) REPO_URL="${2:-}"; shift 2 ;;
    --git-ref) GIT_REF="${2:-}"; shift 2 ;;
    --bind)
      BIND="${2:-}"
      SERVER_PORT="$(port_from_bind "$BIND")"
      if [[ -z "$PUBLIC_BASE_URL" || "$PUBLIC_BASE_URL" == http://127.0.0.1:* ]]; then
        PUBLIC_BASE_URL="http://127.0.0.1:$SERVER_PORT"
      fi
      shift 2
      ;;
    --public-base-url) PUBLIC_BASE_URL="${2:-}"; shift 2 ;;
    --api-key-env) API_KEY_ENV="${2:-}"; shift 2 ;;
    --db-port) DB_PORT="${2:-}"; shift 2 ;;
    --builder-image) BUILDER_IMAGE="${2:-}"; shift 2 ;;
    --base-image) BASE_IMAGE="${2:-}"; shift 2 ;;
    --test-image) TEST_IMAGE="${2:-}"; shift 2 ;;
    --print) PRINT_CONFIG=true; shift ;;
    --force) FORCE=true; shift ;;
    --skip-git-probe) STRICT_GIT=false; shift ;;
    -h|--help) usage; exit 0 ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$APP_DIR" ]]; then
  APP_DIR="${LOGAGENT_APP_DIR:-}"
fi
if [[ -z "$APP_DIR" ]]; then
  APP_DIR="$(read_export_value LOGAGENT_APP_DIR "$BASHRC_FILE" 2>/dev/null || true)"
fi
if [[ -z "$SRC_DIR" ]]; then
  SRC_DIR="${LOGAGENT_SRC_DIR:-}"
fi
if [[ -z "$SRC_DIR" ]]; then
  SRC_DIR="$(read_export_value LOGAGENT_SRC_DIR "$BASHRC_FILE" 2>/dev/null || true)"
fi
if [[ -z "$SRC_DIR" && -f "$REPO_ROOT/Cargo.toml" ]]; then
  SRC_DIR="$REPO_ROOT"
fi

if [[ -z "$APP_DIR" ]]; then
  add_error "LOGAGENT_APP_DIR is missing. Export it or add 'export LOGAGENT_APP_DIR=/path' to $BASHRC_FILE."
else
  if [[ -d "$(expand_home "$APP_DIR")" ]]; then
    APP_DIR="$(abs_dir "$APP_DIR")"
  else
    add_error "LOGAGENT_APP_DIR does not exist: $APP_DIR"
  fi
fi

if [[ -z "$SRC_DIR" ]]; then
  add_error "LOGAGENT_SRC_DIR is missing and this script is not inside a repo checkout."
else
  if [[ -d "$(expand_home "$SRC_DIR")" ]]; then
    SRC_DIR="$(abs_dir "$SRC_DIR")"
  else
    add_error "LOGAGENT_SRC_DIR does not exist: $SRC_DIR"
  fi
fi

if [[ -n "$APP_DIR" && -z "$OUTPUT" ]]; then
  OUTPUT="$APP_DIR/deploy/server-influxdb.yaml"
fi
if [[ -n "$APP_DIR" && -z "$DATA_DIR" ]]; then
  DATA_DIR="$APP_DIR/data"
fi
if [[ -n "$DATA_DIR" ]]; then
  DATA_DIR="$(expand_home "$DATA_DIR")"
fi
if [[ -z "$OUTPUT" ]]; then
  add_error "config output path is missing. Set LOGAGENT_APP_DIR or pass --output."
fi
if [[ -z "$DATA_DIR" ]]; then
  add_error "storage data dir is missing. Set LOGAGENT_APP_DIR or pass --data-dir."
fi

SERVER_PORT="$(port_from_bind "$BIND")"

GIT_BIN="$(command -v git || true)"
DOCKER_BIN="$(command -v docker || true)"
CURL_BIN="$(command -v curl || true)"

[[ -n "$GIT_BIN" ]] || add_error "git is missing"
[[ -n "$DOCKER_BIN" ]] || add_error "docker is missing"
[[ -n "$CURL_BIN" ]] || add_error "curl is missing"

case "$DB_PORT" in
  ''|*[!0-9]*) add_error "--db-port must be a numeric port" ;;
  *) if (( DB_PORT < 1 || DB_PORT > 65535 )); then add_error "--db-port must be between 1 and 65535"; fi ;;
esac

if [[ -n "$SRC_DIR" ]]; then
  [[ -f "$SRC_DIR/Cargo.toml" ]] || add_error "LOGAGENT_SRC_DIR does not look like prd_assistant: missing Cargo.toml"
  [[ -x "$SRC_DIR/deploy/devselftest/influxdb/build-influxdb.sh" ]] || add_error "missing executable build script: $SRC_DIR/deploy/devselftest/influxdb/build-influxdb.sh"
  [[ -f "$SRC_DIR/deploy/devselftest/influxdb/docker-compose.yml" ]] || add_error "missing compose file: $SRC_DIR/deploy/devselftest/influxdb/docker-compose.yml"
  [[ -d "$SRC_DIR/deploy/devselftest/influxdb/tests" ]] || add_error "missing tests dir: $SRC_DIR/deploy/devselftest/influxdb/tests"
fi

if [[ -n "$DOCKER_BIN" ]]; then
  if ! "$DOCKER_BIN" compose version >/dev/null 2>&1; then
    add_error "docker compose plugin is unavailable"
  fi
  if ! "$DOCKER_BIN" info >/dev/null 2>&1; then
    add_error "docker daemon is unavailable to the current user"
  fi
  if ! docker_image_available "$BASE_IMAGE"; then
    add_warning "runtime image $BASE_IMAGE is not local; pull it before the first workflow run"
  fi
  if ! docker_image_available "$BUILDER_IMAGE"; then
    add_warning "builder image $BUILDER_IMAGE is not local; pull it before the first workflow run"
  fi
  if ! docker_image_available "$TEST_IMAGE"; then
    add_warning "test image $TEST_IMAGE is not local; pull it before the first workflow run"
  fi
fi

if [[ -n "$GIT_BIN" && "$STRICT_GIT" == true ]]; then
  if ! GIT_SSH_COMMAND="${GIT_SSH_COMMAND:-ssh -o StrictHostKeyChecking=accept-new}" "$GIT_BIN" ls-remote "$REPO_URL" "$GIT_REF" >/dev/null 2>&1; then
    add_error "cannot access git repo/ref: $REPO_URL $GIT_REF"
  fi
fi

if [[ -z "${!API_KEY_ENV:-}" ]]; then
  local_api_key="$(read_export_value "$API_KEY_ENV" "$BASHRC_FILE" 2>/dev/null || true)"
  if [[ -z "$local_api_key" ]]; then
    add_warning "$API_KEY_ENV is not set; Server start will fail until this env var is exported"
  fi
fi

if port_listening "$DB_PORT"; then
  add_error "port $DB_PORT is already listening; the bundled InfluxDB compose maps $DB_PORT:8086"
fi
if port_listening "$SERVER_PORT"; then
  add_warning "server bind port $SERVER_PORT is already listening; generated config may be for an already-running Server"
fi

echo "Probe summary:"
echo "  app dir:       ${APP_DIR:-<missing>}"
echo "  source dir:    ${SRC_DIR:-<missing>}"
echo "  output:        ${OUTPUT:-<missing>}"
echo "  data dir:      ${DATA_DIR:-<missing>}"
echo "  git:           ${GIT_BIN:-<missing>}"
echo "  docker:        ${DOCKER_BIN:-<missing>}"
echo "  curl:          ${CURL_BIN:-<missing>}"
echo "  repo url:      $REPO_URL"
echo "  git ref:       $GIT_REF"
echo "  bind:          $BIND"
echo "  public url:    $PUBLIC_BASE_URL"
echo "  db port:       $DB_PORT"
echo "  builder image: $BUILDER_IMAGE"
echo "  runtime image: $BASE_IMAGE"
echo "  test image:    $TEST_IMAGE"

if [[ ${#warnings[@]} -gt 0 ]]; then
  echo
  echo "Warnings:"
  for warning in "${warnings[@]}"; do
    echo "  - $warning"
  done
fi

if [[ ${#errors[@]} -gt 0 ]]; then
  echo
  echo "Errors:"
  for error in "${errors[@]}"; do
    echo "  - $error"
  done
  if [[ "$FORCE" != true ]]; then
    echo
    echo "Config not written. Fix the errors or rerun with --force." >&2
    exit 1
  fi
  echo
  echo "--force set; writing config despite errors."
fi

mkdir -p "$(dirname "$OUTPUT")" "$DATA_DIR"

GOPROXY_VALUE="${GOPROXY:-https://goproxy.cn,direct}"
GOSUMDB_VALUE="${GOSUMDB:-}"
INSTALL_BUILD_DEPS_VALUE="${INFLUXDB_INSTALL_BUILD_DEPS:-}"
RUST_TOOLCHAIN_VALUE="${INFLUXDB_RUST_TOOLCHAIN:-}"

{
  printf 'server:\n'
  printf '  bind: "%s"\n' "$(yaml_escape "$BIND")"
  printf '  public_base_url: "%s"\n' "$(yaml_escape "$PUBLIC_BASE_URL")"
  printf '  max_concurrent_tasks: 2\n'
  printf '  max_input_chars: 60000\n\n'
  printf 'auth:\n'
  printf '  api_keys:\n'
  printf '    - name: "native-agent"\n'
  printf '      value_env: "%s"\n\n' "$(yaml_escape "$API_KEY_ENV")"
  printf 'storage:\n'
  printf '  data_dir: "%s"\n' "$(yaml_escape "$DATA_DIR")"
  printf '  max_upload_bytes: 2147483648\n'
  printf '  max_chunk_bytes: 524288\n\n'
  printf 'log_analyzer:\n'
  printf '  max_matches: 200\n'
  printf '  keywords: ["error", "exception", "timeout", "fail", "failed", "panic", "fatal"]\n\n'
  printf 'remote_execution:\n'
  printf '  commands:\n'
  printf '    influxdb_smoke:\n'
  printf '      display_name: "InfluxDB smoke (in-container)"\n'
  printf '      description: "Run the InfluxDB v1 HTTP smoke shell script inside the docker test container."\n'
  printf '      enabled: true\n'
  printf '      argv: ["sh", "/tests/smoke.sh"]\n'
  printf '      timeout_seconds: 180\n\n'
  printf 'dev_selftest:\n'
  printf '  enabled: true\n'
  printf '  build_timeout_seconds: 2400\n'
  printf '  max_output_bytes: 8388608\n'
  printf '  git:\n'
  printf '    enabled: true\n'
  printf '    binary: "%s"\n' "$(yaml_escape "$GIT_BIN")"
  printf '    repos:\n'
  printf '      - url: "%s"\n' "$(yaml_escape "$REPO_URL")"
  printf '        refs: ["%s"]\n' "$(yaml_escape "$GIT_REF")"
  printf '  builds:\n'
  printf '    influxdb:\n'
  printf '      display_name: "InfluxDB single-node server"\n'
  printf '      argv: ["bash", "/scripts/build-influxdb.sh"]\n'
  printf '      working_dir: ""\n'
  printf '      artifact_globs: ["build/influxd"]\n'
  printf '      timeout_seconds: 2400\n'
  printf '      docker:\n'
  printf '        image: "%s"\n' "$(yaml_escape "$BUILDER_IMAGE")"
  printf '        network: "host"\n'
  printf '        workdir: "/workspace/source"\n'
  printf '        volumes:\n'
  printf '          - "%s/deploy/devselftest/influxdb:/scripts:ro"\n' "$(yaml_escape "$SRC_DIR")"
  printf '        env:\n'
  printf '          GOPROXY: "%s"\n' "$(yaml_escape "$GOPROXY_VALUE")"
  if [[ -n "$GOSUMDB_VALUE" ]]; then
    printf '          GOSUMDB: "%s"\n' "$(yaml_escape "$GOSUMDB_VALUE")"
  fi
  if [[ -n "$INSTALL_BUILD_DEPS_VALUE" ]]; then
    printf '          INFLUXDB_INSTALL_BUILD_DEPS: "%s"\n' "$(yaml_escape "$INSTALL_BUILD_DEPS_VALUE")"
  fi
  if [[ -n "$RUST_TOOLCHAIN_VALUE" ]]; then
    printf '          INFLUXDB_RUST_TOOLCHAIN: "%s"\n' "$(yaml_escape "$RUST_TOOLCHAIN_VALUE")"
  fi
  printf '  docker:\n'
  printf '    binary: "%s"\n' "$(yaml_escape "$DOCKER_BIN")"
  printf '    clusters:\n'
  printf '      influxdb_single:\n'
  printf '        compose_file: "%s/deploy/devselftest/influxdb/docker-compose.yml"\n' "$(yaml_escape "$SRC_DIR")"
  printf '        exposed_port: %s\n' "$DB_PORT"
  printf '        health_check:\n'
  printf '          cmd: ["%s", "-sf", "http://127.0.0.1:%s/query?q=SHOW+DATABASES"]\n' "$(yaml_escape "$CURL_BIN")" "$DB_PORT"
  printf '          timeout_seconds: 180\n'
  printf '  test_suites:\n'
  printf '    influxdb_smoke:\n'
  printf '      command: influxdb_smoke\n'
  printf '      timeout_seconds: 180\n'
  printf '      env: {}\n'
  printf '      docker:\n'
  printf '        image: "%s"\n' "$(yaml_escape "$TEST_IMAGE")"
  printf '        network: "host"\n'
  printf '        volumes:\n'
  printf '          - "%s/deploy/devselftest/influxdb/tests:/tests:ro"\n\n' "$(yaml_escape "$SRC_DIR")"
  printf 'mcp:\n'
  printf '  enabled: true\n'
  printf '  transport: "stdio"\n'
} > "$OUTPUT"

echo
echo "Wrote config: $OUTPUT"
if [[ "$PRINT_CONFIG" == true ]]; then
  echo
  cat "$OUTPUT"
fi
