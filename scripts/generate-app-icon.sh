#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="${AETOWER_APP_ICON_BUILD_DIR:-$ROOT/tmp/app-icon}"
ICONSET_PATH="$OUTPUT_DIR/Aetower.iconset"
ICNS_PATH="${AETOWER_APP_ICON_PATH:-$OUTPUT_DIR/Aetower.icns}"
SOURCE_ICON="${AETOWER_APP_ICON_SOURCE:-$ROOT/assets/brand/aetower-app-icon-source.png}"

if ! command -v iconutil >/dev/null 2>&1; then
    echo "iconutil is required to generate the macOS .icns app icon" >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"
if [ -f "$SOURCE_ICON" ]; then
    mkdir -p "$ICONSET_PATH"
    rm -f "$ICONSET_PATH"/*.png
    sips -z 16 16 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_16x16.png" >/dev/null
    sips -z 32 32 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_16x16@2x.png" >/dev/null
    sips -z 32 32 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_32x32.png" >/dev/null
    sips -z 64 64 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_32x32@2x.png" >/dev/null
    sips -z 128 128 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_128x128.png" >/dev/null
    sips -z 256 256 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_128x128@2x.png" >/dev/null
    sips -z 256 256 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_256x256.png" >/dev/null
    sips -z 512 512 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_256x256@2x.png" >/dev/null
    sips -z 512 512 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_512x512.png" >/dev/null
    sips -z 1024 1024 "$SOURCE_ICON" --out "$ICONSET_PATH/icon_512x512@2x.png" >/dev/null
else
    python3 "$ROOT/scripts/generate-app-icon.py" "$ICONSET_PATH"
fi
rm -f "$ICNS_PATH"
iconutil -c icns -o "$ICNS_PATH" "$ICONSET_PATH"
printf '%s\n' "$ICNS_PATH"
