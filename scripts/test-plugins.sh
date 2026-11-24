#!/usr/bin/env bash
# Manual plugin-hosting integration suite.
#
# Builds the local truce fixture for every format this host can build, then
# runs the ignored `notwig-plugins` integration tests against those bundles.
#
# Prerequisites:
#   cargo install cargo-truce --version 6.3.0
#   A native compiler toolchain (Xcode CLI tools on macOS, build-essential +
#   X11 headers on Linux, MSVC build tools on Windows).
#   LV2 additionally needs the system `lilv` library (pkg-config lilv-0).
#   AU v2 additionally needs macOS; AU v3 also needs full Xcode.
#
# Usage:
#   scripts/test-plugins.sh
#   scripts/test-plugins.sh --no-lv2
#   NOTWIG_PLUGIN_TEST_AU3=1 scripts/test-plugins.sh
#
# The fixture build is intentionally outside the Notwig workspace so truce's
# plugin-side dependency graph never becomes a normal workspace dependency.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="$ROOT/crates/notwig-plugins/tests/fixtures/truce_test_plugin"
TARGET_DIR="${NOTWIG_PLUGIN_FIXTURE_TARGET:-$ROOT/target/plugin-fixtures}"

want_lv2="${NOTWIG_PLUGIN_TEST_LV2:-auto}"
want_au3="${NOTWIG_PLUGIN_TEST_AU3:-0}"
for arg in "$@"; do
  case "$arg" in
    --no-lv2) want_lv2=0 ;;
    --lv2) want_lv2=1 ;;
    --au3) want_au3=1 ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

if ! command -v cargo-truce >/dev/null 2>&1; then
  echo "error: cargo-truce is required; run:" >&2
  echo "  cargo install cargo-truce --version 6.3.0" >&2
  exit 1
fi

formats=(clap vst3 vst2)
features=(clap vst3 vst2)

if [[ "$want_lv2" == "auto" ]]; then
  if pkg-config --exists lilv-0 2>/dev/null; then
    want_lv2=1
  else
    want_lv2=0
    echo "note: lilv-0 not found; skipping LV2 (install liblilv-dev or pass --lv2)" >&2
  fi
fi
if [[ "$want_lv2" == "1" ]]; then
  formats+=(lv2)
  features+=(lv2)
fi

host_os="$(uname -s)"
if [[ "$host_os" == "Darwin" ]]; then
  formats+=(au2)
  features+=(au)
  if [[ "$want_au3" == "1" ]]; then
    formats+=(au3)
    features+=(au3)
  fi
fi

build_args=()
for format in "${formats[@]}"; do
  build_args+=("--$format")
done

echo "==> building truce fixture formats: ${formats[*]}"
(
  cd "$FIXTURE"
  CARGO_TARGET_DIR="$TARGET_DIR" cargo truce build "${build_args[@]}"
)

# AU discovery is registry-based, so the built component must be registered
# with the system before the host can scan it. User scope needs no sudo.
if [[ "$host_os" == "Darwin" ]]; then
  echo "==> installing AU fixture so AudioComponent discovery can see it"
  (
    cd "$FIXTURE"
    CARGO_TARGET_DIR="$TARGET_DIR" cargo truce install --au2 --user --no-build
    if [[ "$want_au3" == "1" ]]; then
      CARGO_TARGET_DIR="$TARGET_DIR" cargo truce install --au3 --no-build
    fi
  )
fi

export NOTWIG_PLUGIN_FIXTURE_BUNDLES="$TARGET_DIR/bundles"
# lilv reads LV2_PATH before it scans and treats every entry as a bundle, so
# point it at a dedicated directory containing only the fixture's `.lv2`
# bundle (the shared bundles dir also holds CLAP/VST artifacts).
if [[ "$want_lv2" == "1" ]]; then
  lv2_path="$TARGET_DIR/lv2-path"
  rm -rf "$lv2_path"
  mkdir -p "$lv2_path"
  shopt -s nullglob
  lv2_bundles=("$NOTWIG_PLUGIN_FIXTURE_BUNDLES"/*.lv2)
  shopt -u nullglob
  if (( ${#lv2_bundles[@]} == 0 )); then
    echo "error: no LV2 bundle built under $NOTWIG_PLUGIN_FIXTURE_BUNDLES" >&2
    exit 1
  fi
  cp -R "${lv2_bundles[@]}" "$lv2_path/"
  export LV2_PATH="$lv2_path${LV2_PATH:+:$LV2_PATH}"
fi

feature_list="$(IFS=,; echo "${features[*]}")"
echo "==> running host integration tests (features: $feature_list)"
cargo test -p notwig-plugins \
  --test plugin_integration \
  --features "$feature_list" \
  -- --ignored --test-threads=1

if [[ "$host_os" == "Darwin" ]]; then
  echo "==> running native editor tests (AppKit windows)"
  cargo test -p notwig-plugins \
    --test plugin_editor \
    --features "$feature_list" \
    -- --ignored --test-threads=1
else
  echo "==> native editor windows are macOS-only; editor stub is covered by the normal suite"
fi
