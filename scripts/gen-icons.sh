#!/usr/bin/env bash
# Generate app icons and tray PNGs from the SVG art in assets/design/.
# Uses the project-local, pinned @tauri-apps/cli (never downloads: --no).
set -euo pipefail

cd "$(dirname "$0")/.."

TRAY_NAMES=(rest awake-0 awake-1 awake-2 awake-3)
TRAY_SIZES=(16 32 64)

# App icons (icns/ico/png set) -> src-tauri/icons/
npm exec --no -- tauri icon assets/design/plant.svg -o src-tauri/icons
# Desktop-only app: drop the mobile icon sets the CLI also writes.
rm -rf src-tauri/icons/android src-tauri/icons/ios

# Tray PNGs. `tauri icon --png <sizes>` takes a comma-separated list of pixel
# sizes and writes only <size>x<size>.png into the output dir (verified with
# @tauri-apps/cli 2.12.4: with --png the default icon set is not generated, so each
# tray source is rendered once, at the three sizes only). Each file is checked before
# it is moved, so a changed CLI fails loudly instead of leaving stale PNGs.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p src-tauri/icons/tray

for name in "${TRAY_NAMES[@]}"; do
  out="$tmp/$name"
  npm exec --no -- tauri icon "assets/design/tray/$name.svg" \
    --png "$(IFS=,; echo "${TRAY_SIZES[*]}")" -o "$out"
  for size in "${TRAY_SIZES[@]}"; do
    png="$out/${size}x${size}.png"
    if [[ ! -f "$png" ]]; then
      echo "gen-icons: tauri icon did not write $png (CLI output format changed?)" >&2
      exit 1
    fi
    mv "$png" "src-tauri/icons/tray/$name-$size.png"
  done
done
