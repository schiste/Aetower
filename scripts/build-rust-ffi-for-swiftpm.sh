#!/bin/sh
set -eu

if [ "$#" -ne 1 ]; then
    echo "usage: $0 <cargo-target-dir>" >&2
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET_DIR="$1"

mkdir -p "$TARGET_DIR"
cd "$ROOT/rust"

CARGO_TARGET_DIR="$TARGET_DIR" cargo build --locked -p aetower-ffi
CARGO_TARGET_DIR="$TARGET_DIR" cargo build --locked -p aetower-ffi --release

install_name_tool -id "@rpath/libaetower_ffi.dylib" "$TARGET_DIR/debug/libaetower_ffi.dylib"
install_name_tool -id "@rpath/libaetower_ffi.dylib" "$TARGET_DIR/release/libaetower_ffi.dylib"
