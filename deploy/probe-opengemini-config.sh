#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BASHRC_FILE="${LOGAGENT_BASHRC_FILE:-$HOME/.bashrc}"

DEFAULT_REPO_URL="ssh://git@github.com/zhiwangdu/openGemini.git"
DEFAULT_GIT_REF="devselftest/go126-sonic-latest-20260625-233438"

APP_DIR=""
SRC_DIR=""
OUTPUT=""
DATA_DIR=""
REPO_URL="${LOGAGENT_OPENGEMINI_REPO_URL:-$DEFAULT_REPO_URL}"
GIT_REF="${LOGAGENT_OPENGEMINI_REF:-$DEFAULT_GIT_REF}"
BIND="${LOGAGENT_BIND:-127.0.0.1:50994}"
PUBLIC_BASE_URL="${LOGAGENT_PUBLIC_BASE_URL:-http://127.0.0.1:50994}"
API_KEY_ENV="${LOGAGENT_API_KEY_ENV:-LOGAGENT_NATIVE_API_KEY}"
SERVER_PORT="50994"
OG_PORT="8086"
PRINT_CONFIG=false
FORCE=false
STRICT_GIT=true

usage() {
  cat <<'USAGE'
Usage:
  deploy/probe-opengemini-config.sh [options]

Probes the local Linux environment and writes an openGemini dev_selftest Server
config. The script reads LOGAGENT_APP_DIR and LOGAGENT_SRC_DIR from the current
environment first, then from ~/.bashrc-style "export NAME=value" lines, so it
works even when non-interactive shells return before loading those exports.

Defaults:
  output    $LOGAGENT_APP_DIR/deploy/server-opengemini.yaml
  data dir  $LOGAGENT_APP_DIR/data
  repo      ssh://git@github.com/zhiwangdu/openGemini.git
  ref       devselftest/go126-sonic-latest-20260625-233438

Options:
  --app-dir DIR          Runtime app dir. Defaults to LOGAGENT_APP_DIR.
  --src-dir DIR          prd_assistant repo dir. Defaults to LOGAGENT_SRC_DIR or this checkout.
  --output FILE          Config output path.
  --data-dir DIR         storage.data_dir value.
  --repo-url URL         Allowed openGemini git repo URL.
  --git-ref REF          Allowed openGemini git ref.
  --bind ADDR:PORT       Server bind address. Default: 127.0.0.1:50994.
  --public-base-url URL  Server public_base_url.
  --api-key-env NAME     Auth env var name. Default: LOGAGENT_NATIVE_API_KEY.
  --print                Print generated config after writing it.
  --force                Write config even if required probes fail.
  --skip-git-probe       Do not run git ls-remote for repo/ref reachability.
  -h, --help             Show this help.

Environment overrides:
  LOGAGENT_OPENGEMINI_REPO_URL, LOGAGENT_OPENGEMINI_REF, LOGAGENT_BIND,
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
    --app-dir)
      APP_DIR="${2:-}"
      shift 2
      ;;
    --src-dir)
      SRC_DIR="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT="${2:-}"
      shift 2
      ;;
    --data-dir)
      DATA_DIR="${2:-}"
      shift 2
      ;;
    --repo-url)
      REPO_URL="${2:-}"
      shift 2
      ;;
    --git-ref)
      GIT_REF="${2:-}"
      shift 2
      ;;
    --bind)
      BIND="${2:-}"
      SERVER_PORT="$(port_from_bind "$BIND")"
      if [[ -z "$PUBLIC_BASE_URL" || "$PUBLIC_BASE_URL" == http://127.0.0.1:* ]]; then
        PUBLIC_BASE_URL="http://127.0.0.1:$SERVER_PORT"
      fi
      shift 2
      ;;
    --public-base-url)
      PUBLIC_BASE_URL="${2:-}"
      shift 2
      ;;
    --api-key-env)
      API_KEY_ENV="${2:-}"
      shift 2
      ;;
    --print)
      PRINT_CONFIG=true
      shift
      ;;
    --force)
      FORCE=true
      shift
      ;;
    --skip-git-probe)
      STRICT_GIT=false
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
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
  OUTPUT="$APP_DIR/deploy/server-opengemini.yaml"
fi
if [[ -n "$APP_DIR" && -z "$DATA_DIR" ]]; then
  DATA_DIR="$APP_DIR/data"
fi
if [[ -n "$DATA_DIR" ]]; then
  DATA_DIR="$(expand_home "$DATA_DIR")"
fi

SERVER_PORT="$(port_from_bind "$BIND")"

GIT_BIN="$(command -v git || true)"
DOCKER_BIN="$(command -v docker || true)"
CURL_BIN="$(command -v curl || true)"

[[ -n "$GIT_BIN" ]] || add_error "git is missing"
[[ -n "$DOCKER_BIN" ]] || add_error "docker is missing"
[[ -n "$CURL_BIN" ]] || add_error "curl is missing"

if [[ -n "$SRC_DIR" ]]; then
  [[ -f "$SRC_DIR/Cargo.toml" ]] || add_error "LOGAGENT_SRC_DIR does not look like prd_assistant: missing Cargo.toml"
  [[ -x "$SRC_DIR/deploy/devselftest/opengemini/build-opengemini.sh" ]] || add_error "missing executable build script: $SRC_DIR/deploy/devselftest/opengemini/build-opengemini.sh"
  [[ -f "$SRC_DIR/deploy/devselftest/opengemini/docker-compose.yml" ]] || add_error "missing compose file: $SRC_DIR/deploy/devselftest/opengemini/docker-compose.yml"
  [[ -d "$SRC_DIR/deploy/devselftest/opengemini/tests" ]] || add_error "missing tests dir: $SRC_DIR/deploy/devselftest/opengemini/tests"
fi

if [[ -n "$DOCKER_BIN" ]]; then
  if ! "$DOCKER_BIN" compose version >/dev/null 2>&1; then
    add_error "docker compose plugin is unavailable"
  fi
  if ! "$DOCKER_BIN" info >/dev/null 2>&1; then
    add_error "docker daemon is unavailable to the current user"
  fi
  if ! "$DOCKER_BIN" image inspect ubuntu:24.04 >/dev/null 2>&1; then
    add_warning "docker image ubuntu:24.04 is not local; pull it before the first workflow run"
  fi
  if ! "$DOCKER_BIN" image inspect alpine:3.20 >/dev/null 2>&1; then
    add_warning "docker image alpine:3.20 is not local; pull it before the first workflow run"
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

if port_listening "$OG_PORT"; then
  add_error "port $OG_PORT is already listening; the bundled openGemini compose maps $OG_PORT:$OG_PORT"
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

cat > "$OUTPUT" <<EOF
server:
  bind: "$(yaml_escape "$BIND")"
  public_base_url: "$(yaml_escape "$PUBLIC_BASE_URL")"
  max_concurrent_tasks: 2
  max_input_chars: 60000

auth:
  api_keys:
    - name: "native-agent"
      value_env: "$(yaml_escape "$API_KEY_ENV")"

storage:
  data_dir: "$(yaml_escape "$DATA_DIR")"
  max_upload_bytes: 2147483648
  max_chunk_bytes: 524288

log_analyzer:
  max_matches: 200
  keywords: ["error", "exception", "timeout", "fail", "failed", "panic", "fatal"]

remote_execution:
  commands:
    opengemini_smoke:
      display_name: "openGemini smoke (in-container)"
      description: "Run the smoke shell script inside the docker test container."
      enabled: true
      argv: ["sh", "/tests/smoke.sh"]
      timeout_seconds: 180

dev_selftest:
  enabled: true
  build_timeout_seconds: 1800
  max_output_bytes: 8388608
  git:
    enabled: true
    binary: "$(yaml_escape "$GIT_BIN")"
    repos:
      - url: "$(yaml_escape "$REPO_URL")"
        refs: ["$(yaml_escape "$GIT_REF")"]
  builds:
    opengemini:
      command: ["$(yaml_escape "$SRC_DIR")/deploy/devselftest/opengemini/build-opengemini.sh"]
      working_dir: ""
      artifact_globs: ["build/ts-meta", "build/ts-store", "build/ts-sql"]
      timeout_seconds: 1800
  docker:
    binary: "$(yaml_escape "$DOCKER_BIN")"
    clusters:
      opengemini_cluster:
        compose_file: "$(yaml_escape "$SRC_DIR")/deploy/devselftest/opengemini/docker-compose.yml"
        exposed_port: 8086
        health_check:
          cmd: ["$(yaml_escape "$CURL_BIN")", "-sf", "http://127.0.0.1:8086/query?q=SHOW+DATABASES"]
          timeout_seconds: 180
  test_suites:
    opengemini_smoke:
      command: opengemini_smoke
      timeout_seconds: 180
      env: {}
      docker:
        image: "alpine:3.20"
        network: "host"
        volumes:
          - "$(yaml_escape "$SRC_DIR")/deploy/devselftest/opengemini/tests:/tests:ro"

mcp:
  enabled: true
  transport: "stdio"
EOF

echo
echo "Wrote config: $OUTPUT"
if [[ "$PRINT_CONFIG" == true ]]; then
  echo
  cat "$OUTPUT"
fi
