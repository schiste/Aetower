#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
ARTIFACT="${AETOWER_RELEASE_ARCHIVE:-$DIST_DIR/Aetower.zip}"
OUTPUT_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"

if [ ! -f "$ARTIFACT" ]; then
    echo "release archive not found: $ARTIFACT" >&2
    exit 1
fi

SPARKLE_BIN=""
for CANDIDATE in \
    "$ROOT/macos/.build/checkouts/Sparkle/bin/generate_appcast" \
    "$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_appcast"
do
    if [ -x "$CANDIDATE" ]; then
        SPARKLE_BIN="$CANDIDATE"
        break
    fi
done

if [ -z "$SPARKLE_BIN" ]; then
    echo "Sparkle generate_appcast tool not found. Build once with the Sparkle package resolved first." >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR"
"$SPARKLE_BIN" --output-dir "$OUTPUT_DIR" "$ARTIFACT"
