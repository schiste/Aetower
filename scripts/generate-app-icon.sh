#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="${AETOWER_APP_ICON_BUILD_DIR:-$ROOT/tmp/app-icon}"
ICONSET_PATH="$OUTPUT_DIR/Aetower.iconset"
ICNS_PATH="${AETOWER_APP_ICON_PATH:-$OUTPUT_DIR/Aetower.icns}"

if ! command -v iconutil >/dev/null 2>&1; then
    echo "iconutil is required to generate the macOS .icns app icon" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"
python3 "$ROOT/scripts/generate-app-icon.py" "$ICONSET_PATH"
rm -f "$ICNS_PATH"
iconutil -c icns -o "$ICNS_PATH" "$ICONSET_PATH"
printf '%s\n' "$ICNS_PATH"
