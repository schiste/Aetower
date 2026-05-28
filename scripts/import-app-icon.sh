#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOURCE_PATH="${1:-}"
BRAND_DIR="$ROOT/assets/brand"
CANONICAL_SOURCE="$BRAND_DIR/aetower-app-icon-source.png"
PREVIEW_PATH="$BRAND_DIR/aetower-app-icon-preview.png"

if [ -z "$SOURCE_PATH" ]; then
    echo "usage: $0 <source-icon.png>" >&2
    echo "imports the source as assets/brand/aetower-app-icon-source.png and regenerates the package icon" >&2
    exit 2
fi

if [ ! -f "$SOURCE_PATH" ]; then
    echo "icon source not found: $SOURCE_PATH" >&2
    exit 1
fi

mkdir -p "$BRAND_DIR"
sips -s format png "$SOURCE_PATH" --out "$CANONICAL_SOURCE" >/dev/null
sips -z 1024 1024 "$CANONICAL_SOURCE" --out "$PREVIEW_PATH" >/dev/null
AETOWER_APP_ICON_SOURCE="$CANONICAL_SOURCE" sh "$ROOT/scripts/generate-app-icon.sh" >/dev/null

printf 'imported app icon source: %s\n' "$CANONICAL_SOURCE"
printf 'wrote 1024px preview: %s\n' "$PREVIEW_PATH"
