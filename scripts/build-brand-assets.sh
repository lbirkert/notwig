#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BRAND="$ROOT/brand"

if ! command -v rsvg-convert >/dev/null 2>&1; then
  echo "error: rsvg-convert is required to rebuild brand assets" >&2
  exit 1
fi

rsvg-convert --width 1024 --height 1024 \
  "$BRAND/notwig-app-icon.svg" \
  --output "$BRAND/notwig-app-icon.png"
rsvg-convert --width 256 \
  "$BRAND/notwig-dark.svg" \
  --output "$BRAND/notwig-dark.png"
rsvg-convert --width 256 \
  "$BRAND/notwig-light.svg" \
  --output "$BRAND/notwig-light.png"

echo "rebuilt brand PNGs in $BRAND"
