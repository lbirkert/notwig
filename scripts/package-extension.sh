#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
EXTENSION="$ROOT/crates/notwig-ext/notwig-ext-devices"

build_target() {
  local target="$1"
  local slug
  case "$target" in
    wasm) slug="wasm" ;;
    macos-aarch64) slug="macos-arm64" ;;
    macos-x86_64) slug="macos-x86_64" ;;
    linux-x86_64) slug="linux-x86_64" ;;
    linux-aarch64) slug="linux-aarch64" ;;
    windows-x86_64) slug="windows-x86_64" ;;
    windows-aarch64) slug="windows-aarch64" ;;
    *)
      echo "package-extension: unsupported target: $target" >&2
      echo "supported targets: wasm macos-aarch64 macos-x86_64 linux-x86_64 linux-aarch64 windows-x86_64 windows-aarch64" >&2
      exit 2
      ;;
  esac

  cargo run \
    --quiet \
    --manifest-path "$ROOT/Cargo.toml" \
    -p notwig-ext-cli \
    -- build --entrypoint "$target" --path "$EXTENSION"

  cargo run \
    --quiet \
    --manifest-path "$ROOT/Cargo.toml" \
    -p notwig-ext-cli \
    -- build --package "$target" \
    --path "$EXTENSION" \
    --out-dir "$ROOT/docs/extensions" \
    --name "ext-devices-$slug"
}

if [ "$#" -eq 0 ]; then
  build_target wasm
  if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ]; then
    build_target macos-aarch64
  fi
else
  build_target "$1"
fi
