#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/rust"

cargo build -p aetower-ffi
cargo build -p aetower-ffi --release

mkdir -p "$ROOT/macos/Sources/AetowerBindings" "$ROOT/macos/Sources/aetower_ffiFFI"
find "$ROOT/macos/Sources/AetowerBindings" -type f ! -name '.gitkeep' -delete
find "$ROOT/macos/Sources/aetower_ffiFFI" -type f ! -name '.gitkeep' -delete

cargo run -p uniffi-bindgen-swift -- \
  --swift-sources \
  target/debug/libaetower_ffi.dylib \
  "$ROOT/macos/Sources/AetowerBindings"

cargo run -p uniffi-bindgen-swift -- \
  --headers \
  target/debug/libaetower_ffi.dylib \
  "$ROOT/macos/Sources/aetower_ffiFFI"

cargo run -p uniffi-bindgen-swift -- \
  --modulemap \
  --module-name aetower_ffiFFI \
  --modulemap-filename module.modulemap \
  target/debug/libaetower_ffi.dylib \
  "$ROOT/macos/Sources/aetower_ffiFFI"
