#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MANIFEST="$ROOT/crates/notwig-ext-devices/extension.json"
ICON="$ROOT/crates/notwig-ext-devices/icon.png"
CRATE="notwig-ext-devices"

build_target() {
  local target="$1"
  local rust_target entrypoint built slug
  case "$target" in
    wasm)
      rust_target="wasm32-unknown-unknown"
      entrypoint="notwig_ext_devices.wasm"
      built="$ROOT/target/$rust_target/release/$entrypoint"
      slug="wasm"
      ;;
    macos-aarch64)
      rust_target="aarch64-apple-darwin"
      entrypoint="notwig_ext_devices.dylib"
      built="$ROOT/target/$rust_target/release/libnotwig_ext_devices.dylib"
      slug="macos-arm64"
      ;;
    macos-x86_64)
      rust_target="x86_64-apple-darwin"
      entrypoint="notwig_ext_devices.dylib"
      built="$ROOT/target/$rust_target/release/libnotwig_ext_devices.dylib"
      slug="macos-x86_64"
      ;;
    linux-x86_64)
      rust_target="x86_64-unknown-linux-gnu"
      entrypoint="notwig_ext_devices.so"
      built="$ROOT/target/$rust_target/release/libnotwig_ext_devices.so"
      slug="linux-x86_64"
      ;;
    linux-aarch64)
      rust_target="aarch64-unknown-linux-gnu"
      entrypoint="notwig_ext_devices.so"
      built="$ROOT/target/$rust_target/release/libnotwig_ext_devices.so"
      slug="linux-aarch64"
      ;;
    windows-x86_64)
      rust_target="x86_64-pc-windows-msvc"
      entrypoint="notwig_ext_devices.dll"
      built="$ROOT/target/$rust_target/release/notwig_ext_devices.dll"
      slug="windows-x86_64"
      ;;
    windows-aarch64)
      rust_target="aarch64-pc-windows-msvc"
      entrypoint="notwig_ext_devices.dll"
      built="$ROOT/target/$rust_target/release/notwig_ext_devices.dll"
      slug="windows-aarch64"
      ;;
    *)
      echo "package-extension: unsupported target: $target" >&2
      echo "supported targets: wasm macos-aarch64 macos-x86_64 linux-x86_64 linux-aarch64 windows-x86_64 windows-aarch64" >&2
      exit 2
      ;;
  esac

  local out="${EXTENSION_OUTPUT:-$ROOT/docs/extensions/ext-devices-$slug.zip}"
  cargo build \
    --manifest-path "$ROOT/Cargo.toml" \
    -p "$CRATE" \
    --target "$rust_target" \
    --release

  if [ ! -f "$built" ]; then
    echo "package-extension: build output not found: $built" >&2
    exit 1
  fi
  mkdir -p "$(dirname "$out")"
  python3 - "$out" "$MANIFEST" "$entrypoint" "$built" "$ICON" "$target" <<'PY'
import json
import sys
import zipfile
from pathlib import Path

out = Path(sys.argv[1])
manifest_path = Path(sys.argv[2])
entrypoint = sys.argv[3]
built = Path(sys.argv[4])
icon = Path(sys.argv[5])
target = sys.argv[6]
manifest_text = manifest_path.read_text()
manifest_text = manifest_text.replace("$$TARGET", target)
manifest_text = manifest_text.replace("$$ENTRYPOINT", entrypoint)
manifest = json.loads(manifest_text)
if "$$" in manifest_text:
    raise SystemExit("package-extension: unsubstituted manifest placeholder")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.writestr("extension.json", json.dumps(manifest, indent=2) + "\n")
    archive.write(built, entrypoint)
    if icon.is_file():
        archive.write(icon, "icon.png")
print(f"packaged {out}")
PY
}

if [ "$#" -eq 0 ]; then
  build_target wasm
  if [ "$(uname -s)" = "Darwin" ] && [ "$(uname -m)" = "arm64" ]; then
    build_target macos-aarch64
  fi
else
  build_target "$1"
fi
