#!/usr/bin/env bash
# Generate app icons and tray PNGs from the SVG art in assets/design/.
# Uses the project-local, pinned @tauri-apps/cli (never downloads: --no).
set -euo pipefail

cd "$(dirname "$0")/.."

TRAY_NAMES=(rest awake-0 awake-1 awake-2 awake-3)
TRAY_SIZES=(16 32 64)

# App icons (icns/ico/png set) -> src-tauri/icons/
npm exec --no -- tauri icon assets/design/plant.svg -o src-tauri/icons

# Tray PNGs. NOTE: `tauri icon --png <sizes>` takes a comma-separated list of
# pixel sizes and writes <size>x<size>.png into the output dir (plus the usual
# icon set, which we discard). If the flag/output naming differs in the pinned
# CLI version, adjust here (Task 11 runs this for the first time).
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p src-tauri/icons/tray

for name in "${TRAY_NAMES[@]}"; do
  out="$tmp/$name"
  npm exec --no -- tauri icon "assets/design/tray/$name.svg" \
    --png "$(IFS=,; echo "${TRAY_SIZES[*]}")" -o "$out"
  for size in "${TRAY_SIZES[@]}"; do
    mv "$out/${size}x${size}.png" "src-tauri/icons/tray/$name-$size.png"
  done
done
