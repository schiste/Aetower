#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <plugin-output-dir>" >&2
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="$1"
found=0
for profile in debug release; do
    dylib="$ROOT/rust/target/$profile/libaetower_ffi.dylib"
    if [ -f "$dylib" ]; then
        found=1
        mkdir -p "$OUTPUT_DIR/$profile"
        destination="$OUTPUT_DIR/$profile/libaetower_ffi.dylib"
        if ! cmp -s "$dylib" "$destination"; then
            cp "$dylib" "$destination"
        fi
    fi
done
if [ "$found" = 0 ]; then
    echo "Rust bridge artifacts are missing; run scripts/build-rust.sh [--profile release] before swift build." >&2
    exit 1
fi
# Older build directories use this marker; keep its mtime stable too.
[ -f "$OUTPUT_DIR/.plugin-ready" ] || touch "$OUTPUT_DIR/.plugin-ready"
