#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_BIN="${CARGO_BIN:-$(command -v cargo || printf '%s' cargo)}"
if [ -n "${HOME:-}" ] \
    && [ -x "$HOME/.cargo/bin/cargo" ] \
    && [ "$CARGO_BIN" = "$HOME/.chau7/cto_bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
fi
cd "$ROOT/rust"

"$CARGO_BIN" build --locked -p aetower-ffi
"$CARGO_BIN" build --locked -p aetower-ffi --release

DEBUG_DYLIB="$ROOT/rust/target/debug/libaetower_ffi.dylib"
RELEASE_DYLIB="$ROOT/rust/target/release/libaetower_ffi.dylib"

install_name_tool -id "@rpath/libaetower_ffi.dylib" "$DEBUG_DYLIB"
install_name_tool -id "@rpath/libaetower_ffi.dylib" "$RELEASE_DYLIB"

mkdir -p "$ROOT/macos/Sources/AetowerBindings" "$ROOT/macos/Sources/aetower_ffiFFI"
find "$ROOT/macos/Sources/AetowerBindings" -type f ! -name '.gitkeep' -delete
find "$ROOT/macos/Sources/aetower_ffiFFI" -type f ! -name '.gitkeep' -delete

"$CARGO_BIN" run --locked -p uniffi-bindgen-swift -- \
  --swift-sources \
  "$DEBUG_DYLIB" \
  "$ROOT/macos/Sources/AetowerBindings"

"$CARGO_BIN" run --locked -p uniffi-bindgen-swift -- \
  --headers \
  "$DEBUG_DYLIB" \
  "$ROOT/macos/Sources/aetower_ffiFFI"

"$CARGO_BIN" run --locked -p uniffi-bindgen-swift -- \
  --modulemap \
  --module-name aetower_ffiFFI \
  --modulemap-filename module.modulemap \
  "$DEBUG_DYLIB" \
  "$ROOT/macos/Sources/aetower_ffiFFI"
