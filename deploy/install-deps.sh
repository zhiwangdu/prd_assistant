#!/usr/bin/env bash
set -euo pipefail

DRY_RUN=false
NO_RUSTUP=false

usage() {
  cat <<'USAGE'
Usage: ./install-deps.sh [--dry-run] [--no-rustup]

Installs common dependencies needed by deploy/rebuild-v2-install.sh:
  - git, curl, ca-certificates
  - C/C++ build tools and pkg-config
  - nodejs and npm
  - Go toolchain for source-referenced diagnostic tools
  - Rust toolchain through rustup when cargo is missing, required only when
    building Flux or InfluxDB source-built analyzer tools

Options:
  --dry-run     Print commands without executing them.
  --no-rustup   Do not install Rust automatically; only report whether cargo exists.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=true
      ;;
    --no-rustup)
      NO_RUSTUP=true
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

run() {
  echo "+ $*"
  if [[ "$DRY_RUN" == false ]]; then
    "$@"
  fi
}

sudo_cmd() {
  if [[ "$(id -u)" -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

run_sudo() {
  echo "+ sudo $*"
  if [[ "$DRY_RUN" == false ]]; then
    sudo_cmd "$@"
  fi
}

have() {
  command -v "$1" >/dev/null 2>&1
}

install_linux_deps() {
  if have apt-get; then
    run_sudo apt-get update
    run_sudo apt-get install -y curl ca-certificates git build-essential pkg-config nodejs npm golang-go
    return
  fi
  if have dnf; then
    run_sudo dnf install -y curl ca-certificates git gcc gcc-c++ make pkgconf-pkg-config nodejs npm golang
    return
  fi
  if have yum; then
    run_sudo yum install -y curl ca-certificates git gcc gcc-c++ make pkgconfig nodejs npm golang
    return
  fi
  if have pacman; then
    run_sudo pacman -Sy --needed --noconfirm curl ca-certificates git base-devel pkgconf nodejs npm go
    return
  fi
  echo "Unsupported Linux package manager. Install git, curl, C/C++ build tools, pkg-config, nodejs, npm, Go, and Rust manually." >&2
  exit 1
}

install_macos_deps() {
  if ! xcode-select -p >/dev/null 2>&1; then
    echo "Xcode Command Line Tools are required. Starting installer..."
    run xcode-select --install || true
  fi
  if have brew; then
    run brew update
    run brew install git node pkg-config go
  else
    echo "Homebrew is not installed. Install git, node/npm, pkg-config, Go, and Rust manually, or install Homebrew first." >&2
    exit 1
  fi
}

install_rust_if_needed() {
  if have cargo; then
    echo "cargo already installed: $(cargo --version)"
    return
  fi
  if [[ "$NO_RUSTUP" == true ]]; then
    echo "cargo is missing and --no-rustup was set." >&2
    return
  fi
  if ! have curl; then
    echo "curl is required to install Rust through rustup." >&2
    exit 1
  fi
  echo "Installing Rust through rustup minimal profile..."
  if [[ "$DRY_RUN" == true ]]; then
    echo "+ curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal"
  else
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    export PATH="$HOME/.cargo/bin:$PATH"
  fi
}

case "$(uname -s)" in
  Linux)
    install_linux_deps
    ;;
  Darwin)
    install_macos_deps
    ;;
  *)
    echo "Unsupported OS: $(uname -s). Install dependencies manually." >&2
    exit 1
    ;;
esac

install_rust_if_needed

echo
echo "Dependency check:"
for command_name in git curl node npm go cargo; do
  if have "$command_name"; then
    echo "  ok  $command_name: $($command_name --version 2>/dev/null | head -n 1)"
  else
    echo "  missing  $command_name"
  fi
done

echo
echo "Optional tools:"
if have rg; then
  echo "  ok  rg: $(rg --version | head -n 1)"
else
  echo "  optional missing  rg"
fi
echo "  source-built analyzers are built by scripts/build-tools.sh from third_party/ submodules."
