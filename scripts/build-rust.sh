#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/rust"

cargo build --locked -p aetower-ffi
cargo build --locked -p aetower-ffi --release

DEBUG_DYLIB="$ROOT/rust/target/debug/libaetower_ffi.dylib"
RELEASE_DYLIB="$ROOT/rust/target/release/libaetower_ffi.dylib"

install_name_tool -id "@rpath/libaetower_ffi.dylib" "$DEBUG_DYLIB"
install_name_tool -id "@rpath/libaetower_ffi.dylib" "$RELEASE_DYLIB"

mkdir -p "$ROOT/macos/Sources/AetowerBindings" "$ROOT/macos/Sources/aetower_ffiFFI"
find "$ROOT/macos/Sources/AetowerBindings" -type f ! -name '.gitkeep' -delete
find "$ROOT/macos/Sources/aetower_ffiFFI" -type f ! -name '.gitkeep' -delete

cargo run --locked -p uniffi-bindgen-swift -- \
  --swift-sources \
  "$DEBUG_DYLIB" \
  "$ROOT/macos/Sources/AetowerBindings"

cargo run --locked -p uniffi-bindgen-swift -- \
  --headers \
  "$DEBUG_DYLIB" \
  "$ROOT/macos/Sources/aetower_ffiFFI"

cargo run --locked -p uniffi-bindgen-swift -- \
  --modulemap \
  --module-name aetower_ffiFFI \
  --modulemap-filename module.modulemap \
  "$DEBUG_DYLIB" \
  "$ROOT/macos/Sources/aetower_ffiFFI"
