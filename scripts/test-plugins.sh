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
#   NOTWIG_PLUGIN_TEST_EDITOR=0 scripts/test-plugins.sh   # skip macOS editor windows
#   scripts/test-plugins.sh --editor-only                 # only macOS editor windows
#
# The fixture build is intentionally outside the Notwig workspace so truce's
# plugin-side dependency graph never becomes a normal workspace dependency.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXTURE="$ROOT/crates/notwig-plugins/tests/fixtures/truce_test_plugin"
TARGET_DIR="${NOTWIG_PLUGIN_FIXTURE_TARGET:-$ROOT/target/plugin-fixtures}"

# Native tools (cargo, cargo-truce, the test process) need Windows-style paths
# under Git Bash; shell file operations keep using the POSIX form.
native_path() {
  if command -v cygpath >/dev/null 2>&1; then
    cygpath -m "$1"
  else
    printf '%s' "$1"
  fi
}
TARGET_DIR_NATIVE="$(native_path "$TARGET_DIR")"

want_lv2="${NOTWIG_PLUGIN_TEST_LV2:-auto}"
want_au3="${NOTWIG_PLUGIN_TEST_AU3:-0}"
want_editor="${NOTWIG_PLUGIN_TEST_EDITOR:-1}"
editor_only=0
for arg in "$@"; do
  case "$arg" in
    --no-lv2) want_lv2=0 ;;
    --lv2) want_lv2=1 ;;
    --au3) want_au3=1 ;;
    --editor-only) editor_only=1 ;;
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
if [[ "$editor_only" == "1" ]]; then
  want_editor=1
fi
export NOTWIG_PLUGIN_TEST_EDITOR="$want_editor"

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
  CARGO_TARGET_DIR="$TARGET_DIR_NATIVE" cargo truce build "${build_args[@]}"
)

# AU discovery is registry-based, so the built component must be registered
# with the system before the host can scan it. User scope needs no sudo.
if [[ "$host_os" == "Darwin" ]]; then
  echo "==> installing AU fixture so AudioComponent discovery can see it"
  (
    cd "$FIXTURE"
    CARGO_TARGET_DIR="$TARGET_DIR_NATIVE" cargo truce install --au2 --user --no-build
    if [[ "$want_au3" == "1" ]]; then
      CARGO_TARGET_DIR="$TARGET_DIR_NATIVE" cargo truce install --au3 --no-build
    fi
  )

  au_component="$HOME/Library/Audio/Plug-Ins/Components/Notwig Test Plugin.component"
  if [[ ! -d "$au_component" ]]; then
    echo "error: AU v2 fixture is missing at $au_component" >&2
    exit 1
  fi

  # Cargo-truce clears the AU cache, but the registrar daemon can keep a stale
  # view of installed components (especially on a fresh CI runner). Restart it
  # and give it a moment to rebuild before the host scans the registry.
  killall -9 AudioComponentRegistrar 2>/dev/null \
    || sudo killall -9 AudioComponentRegistrar 2>/dev/null \
    || true
  for _ in 1 2 3 4 5; do
    if auval -a 2>/dev/null | grep -Fq "Notwig Test Plugin"; then
      break
    fi
    sleep 1
  done
  if ! auval -a 2>/dev/null | grep -Fq "Notwig Test Plugin"; then
    echo "warning: AU fixture is not listed by auval yet; continuing so the host scan can report details" >&2
    auval -a 2>/dev/null | grep -iE 'notwig|truce|aufx' | tail -20 >&2 || true
  fi
fi

bundles_dir="$TARGET_DIR/bundles"
export NOTWIG_PLUGIN_FIXTURE_BUNDLES="$(native_path "$bundles_dir")"
# lilv reads LV2_PATH before it scans and treats every entry as a bundle, so
# point it at a dedicated directory containing only the fixture's `.lv2`
# bundle (the shared bundles dir also holds CLAP/VST artifacts).
if [[ "$want_lv2" == "1" ]]; then
  lv2_path="$TARGET_DIR/lv2-path"
  rm -rf "$lv2_path"
  mkdir -p "$lv2_path"
  shopt -s nullglob
  lv2_bundles=("$bundles_dir"/*.lv2)
  shopt -u nullglob
  if (( ${#lv2_bundles[@]} == 0 )); then
    echo "error: no LV2 bundle built under $bundles_dir" >&2
    exit 1
  fi
  cp -R "${lv2_bundles[@]}" "$lv2_path/"
  export LV2_PATH="$(native_path "$lv2_path")${LV2_PATH:+:$LV2_PATH}"
fi

feature_list="$(IFS=,; echo "${features[*]}")"
run_editor_tests() {
  echo "==> running native editor tests (AppKit windows, main thread)"
  cargo test -p notwig-plugins \
    --test plugin_editor \
    --features "$feature_list"
}

if [[ "$editor_only" == "1" ]]; then
  if [[ "$host_os" != "Darwin" ]]; then
    echo "==> native editor tests are macOS-only; nothing to run"
    exit 0
  fi
  run_editor_tests
  exit 0
fi

echo "==> running host integration tests (features: $feature_list)"
cargo test -p notwig-plugins \
  --test plugin_integration \
  --features "$feature_list" \
  -- --ignored --test-threads=1

if [[ "$host_os" == "Darwin" && "$want_editor" == "1" ]]; then
  run_editor_tests
elif [[ "$host_os" == "Darwin" ]]; then
  echo "==> native editor tests skipped (NOTWIG_PLUGIN_TEST_EDITOR=0)"
else
  echo "==> native editor windows are macOS-only; editor stub is covered by the normal suite"
fi
