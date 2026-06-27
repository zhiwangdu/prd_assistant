#!/usr/bin/env bash
# Build the OSS InfluxDB single-node server binary for dev_selftest.
# Runs with cwd = the synced InfluxDB source root.
#
# The default dev_selftest profile runs this script inside a Linux Go builder
# container, so the produced build/influxd binary can be mounted into the
# runtime container. Linux hosts may also run it directly if Go 1.26 + cgo are
# available.
#
# Configurable via the Server process / Docker profile env:
#   GOPROXY  Go module proxy (default https://goproxy.cn,direct).
#   GOSUMDB  Go checksum DB. Set GOSUMDB=off for sealed intranet proxies.
#   PKG_CONFIG  InfluxDB's libflux-aware pkg-config helper. Defaults to
#               ./pkg-config.sh from the checked-out InfluxDB repo.
#   INFLUXDB_INSTALL_BUILD_DEPS  Set to 0 to disable best-effort apt install of
#               curl/pkg-config in minimal Debian/Ubuntu builder images.
#   INFLUXDB_RUST_TOOLCHAIN  Rust toolchain for Flux libflux (default 1.83.0).
set -euo pipefail

export GOPROXY="${GOPROXY:-https://goproxy.cn,direct}"
export PKG_CONFIG="${PKG_CONFIG:-$PWD/pkg-config.sh}"
export INFLUXDB_RUST_TOOLCHAIN="${INFLUXDB_RUST_TOOLCHAIN:-1.83.0}"

ensure_build_deps() {
  local missing_base=()
  command -v curl >/dev/null 2>&1 || missing_base+=("curl")
  command -v pkg-config >/dev/null 2>&1 || missing_base+=("pkg-config")

  if [[ ${#missing_base[@]} -gt 0 ]]; then
    if [[ "${INFLUXDB_INSTALL_BUILD_DEPS:-1}" != "1" ]]; then
      echo "missing build dependencies: ${missing_base[*]}" >&2
      echo "install them in the builder image or set INFLUXDB_INSTALL_BUILD_DEPS=1" >&2
      exit 1
    fi
    if [[ "$(id -u)" != "0" || ! -x "$(command -v apt-get || true)" ]]; then
      echo "missing build dependencies: ${missing_base[*]}" >&2
      echo "automatic install requires root in a Debian/Ubuntu builder with apt-get" >&2
      exit 1
    fi

    export DEBIAN_FRONTEND=noninteractive
    apt-get update
    apt-get install -y --no-install-recommends ca-certificates curl pkg-config
    rm -rf /var/lib/apt/lists/*
  fi

  if rustc_meets_minimum && command -v cargo >/dev/null 2>&1; then
    return 0
  fi

  install_rust_toolchain
}

rustc_meets_minimum() {
  command -v rustc >/dev/null 2>&1 || return 1
  local version major minor
  version="$(rustc --version | awk '{print $2}')"
  major="${version%%.*}"
  minor="${version#*.}"
  minor="${minor%%.*}"
  [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ ]] || return 1
  (( major > 1 || (major == 1 && minor >= 83) ))
}

install_rust_toolchain() {
  if [[ "${INFLUXDB_INSTALL_BUILD_DEPS:-1}" != "1" ]]; then
    echo "rustc/cargo >= 1.83 is required for Flux libflux" >&2
    echo "install Rust in the builder image or set INFLUXDB_INSTALL_BUILD_DEPS=1" >&2
    exit 1
  fi
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl is required to install Rust with rustup" >&2
    exit 1
  fi
  export RUSTUP_HOME="${RUSTUP_HOME:-/tmp/rustup}"
  export CARGO_HOME="${CARGO_HOME:-/tmp/cargo}"
  export PATH="$CARGO_HOME/bin:$PATH"
  if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
      sh -s -- -y --profile minimal --default-toolchain "$INFLUXDB_RUST_TOOLCHAIN"
  else
    rustup toolchain install "$INFLUXDB_RUST_TOOLCHAIN" --profile minimal
    rustup default "$INFLUXDB_RUST_TOOLCHAIN"
  fi
  rustc --version
  cargo --version
}

if [[ ! -x "$PKG_CONFIG" ]]; then
  chmod +x "$PKG_CONFIG"
fi

ensure_build_deps

commit="$(git rev-parse --short HEAD 2>/dev/null || printf 'unknown')"
branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'unknown')"
version="${INFLUXDB_BUILD_VERSION:-dev-selftest}"
tags="${INFLUXDB_BUILD_TAGS:-}"

mkdir -p build

ldflags="-X main.version=${version} -X main.commit=${commit} -X main.branch=${branch}"
args=()
if [[ -n "$tags" ]]; then
  args+=("-tags" "$tags")
fi
args+=("-ldflags" "$ldflags" "-o" "build/influxd" "./cmd/influxd")

set -x
go version
go build "${args[@]}"
set +x

echo "=== build output ==="
ls -la build/influxd
build/influxd version || true
