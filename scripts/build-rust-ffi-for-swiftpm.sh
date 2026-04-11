#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <plugin-output-dir>" >&2
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT_DIR="$1"

DEBUG_DYLIB="$ROOT/rust/target/debug/libaetower_ffi.dylib"
RELEASE_DYLIB="$ROOT/rust/target/release/libaetower_ffi.dylib"

if [ ! -f "$DEBUG_DYLIB" ] || [ ! -f "$RELEASE_DYLIB" ]; then
    echo "Rust bridge artifacts are missing; run scripts/build-rust.sh before swift build." >&2
    exit 1
fi

mkdir -p "$OUTPUT_DIR/debug" "$OUTPUT_DIR/release"
cp "$DEBUG_DYLIB" "$OUTPUT_DIR/debug/libaetower_ffi.dylib"
cp "$RELEASE_DYLIB" "$OUTPUT_DIR/release/libaetower_ffi.dylib"
touch "$OUTPUT_DIR/.plugin-ready"
