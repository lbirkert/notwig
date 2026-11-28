#!/usr/bin/env bash
# Cross-format parity scan for installed plugins.
#
# Scans the normal macOS plugin locations, groups the same plugin across the
# formats that are installed, compares native editor size/content, randomizes
# matched parameters, renders a generated clip through each format, aligns by
# reported latency, and writes a JSON report plus failure artifacts.
#
# Usage:
#   scripts/test-plugin-parity.sh
#   scripts/test-plugin-parity.sh --filter=pro
#   scripts/test-plugin-parity.sh --formats=clap,vst3 --rounds=1
#   scripts/test-plugin-parity.sh --no-gui --audio-tolerance=0.005
#   scripts/test-plugin-parity.sh --au3
#   NOTWIG_PLUGIN_PARITY_FEATURES=clap,vst3,lv2 scripts/test-plugin-parity.sh
#
# Requirements:
#   macOS for native editor windows.
#   LV2 additionally needs the system `lilv` library (pkg-config lilv-0).
#   AU v3 is opt-in because it needs a signed installed component.
#
# The report defaults to target/plugin-parity/run-<timestamp>/report.json.
# Set NOTWIG_PLUGIN_PARITY_DIR or pass --report/--artifacts to choose another
# location. Other arguments are forwarded to the harness, for example:
#   --filter=TEXT --formats=clap,vst3 --rounds=N --seconds=N
#   --audio-tolerance=F --gui-mean-tolerance=F --no-gui --no-audio
#   --include-instruments --ignore-latency --keep-all-artifacts --max-groups=N
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

run_dir="${NOTWIG_PLUGIN_PARITY_DIR:-$ROOT/target/plugin-parity/run-$(date +%Y%m%d-%H%M%S)}"
report="${NOTWIG_PLUGIN_PARITY_REPORT:-$run_dir/report.json}"
artifacts="${NOTWIG_PLUGIN_PARITY_ARTIFACTS:-$run_dir/artifacts}"
mkdir -p "$run_dir"

want_lv2="${NOTWIG_PLUGIN_PARITY_LV2:-auto}"
want_au3="${NOTWIG_PLUGIN_PARITY_AU3:-0}"
forward=()
for arg in "$@"; do
  case "$arg" in
    --no-lv2) want_lv2=0 ;;
    --lv2) want_lv2=1 ;;
    --au3) want_au3=1 ;;
    -h|--help)
      sed -n '2,26p' "$0"
      exit 0
      ;;
    *)
      forward+=("$arg")
      ;;
  esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: plugin format parity with native editors is macOS-only" >&2
  echo "       (the harness still runs off macOS for audio, but editor capture is unavailable)" >&2
  exit 1
fi

if [[ -n "${NOTWIG_PLUGIN_PARITY_FEATURES:-}" ]]; then
  feature_list="$NOTWIG_PLUGIN_PARITY_FEATURES"
else
  features=(clap vst3 vst2)
  if [[ "$want_lv2" == "auto" ]]; then
    if pkg-config --exists lilv-0 2>/dev/null; then
      features+=(lv2)
    else
      echo "note: lilv-0 not found; skipping LV2 (install liblilv-dev or pass --lv2)" >&2
    fi
  elif [[ "$want_lv2" == "1" ]]; then
    features+=(lv2)
  fi
  features+=(au)
  if [[ "$want_au3" == "1" ]]; then
    features+=(au3)
  fi
  feature_list="$(IFS=,; echo "${features[*]}")"
fi

export NOTWIG_PLUGIN_PARITY=1
export NOTWIG_PLUGIN_PARITY_REPORT="$report"
export NOTWIG_PLUGIN_PARITY_ARTIFACTS="$artifacts"
export NOTWIG_PLUGIN_PARITY_DIR="$run_dir"

echo "==> scanning installed plugins (features: $feature_list)"
echo "==> report: $report"
echo "==> artifacts: $artifacts"
cargo test -p notwig-plugins \
  --test plugin_format_parity \
  --features "$feature_list" \
  -- "${forward[@]}"
