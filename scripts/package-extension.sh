#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${EXTENSION_OUTPUT:-$ROOT/docs/extensions/notwig.ext.devices.zip}"
WASM="$ROOT/target/wasm32-unknown-unknown/release/notwig_ext_devices.wasm"
MANIFEST="$ROOT/crates/notwig-ext-devices/extension.json"

cargo build \
  --manifest-path "$ROOT/Cargo.toml" \
  -p notwig-ext-devices \
  --target wasm32-unknown-unknown \
  --release

mkdir -p "$(dirname "$OUT")"
ICON="$ROOT/crates/notwig-ext-devices/icon.png"
python3 - "$OUT" "$MANIFEST" "$WASM" "$ICON" <<'PY'
import sys
import zipfile
from pathlib import Path

out, manifest, wasm, icon = map(Path, sys.argv[1:])
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
    archive.write(manifest, "extension.json")
    archive.write(wasm, "notwig_ext_devices.wasm")
    if icon.is_file():
        archive.write(icon, "icon.png")
print(f"packaged {out}")
PY
