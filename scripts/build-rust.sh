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

"$CARGO_BIN" build --locked -p aetower-mcp
"$CARGO_BIN" build --locked -p aetower-mcp --release
"$CARGO_BIN" build --locked -p aetower-cli
"$CARGO_BIN" build --locked -p aetower-cli --release
"$CARGO_BIN" build --locked -p aetower-ffi
"$CARGO_BIN" build --locked -p aetower-ffi --release

DEBUG_DYLIB="$ROOT/rust/target/debug/libaetower_ffi.dylib"
RELEASE_DYLIB="$ROOT/rust/target/release/libaetower_ffi.dylib"

set_install_name() {
  dylib_path="$1"
  attempts=0
  while [ "$attempts" -lt 5 ]; do
    if [ -f "$dylib_path" ] && install_name_tool -id "@rpath/libaetower_ffi.dylib" "$dylib_path"; then
      return 0
    fi
    attempts=$((attempts + 1))
    sleep 1
  done
  printf 'failed to update install name for %s\n' "$dylib_path" >&2
  return 1
}

set_install_name "$DEBUG_DYLIB"
set_install_name "$RELEASE_DYLIB"

mkdir -p "$ROOT/macos/Sources/AetowerBindings" "$ROOT/macos/Sources/aetower_ffiFFI"
find "$ROOT/macos/Sources/AetowerBindings" -type f ! -name '.gitkeep' -delete
find "$ROOT/macos/Sources/aetower_ffiFFI" -type f ! -name '.gitkeep' -delete

find "$ROOT/macos" -type d -path '*/plugins/outputs/*/BuildRustBridgePlugin' | while IFS= read -r plugin_output_dir; do
  mkdir -p "$plugin_output_dir/debug" "$plugin_output_dir/release"
  cp "$DEBUG_DYLIB" "$plugin_output_dir/debug/libaetower_ffi.dylib"
  cp "$RELEASE_DYLIB" "$plugin_output_dir/release/libaetower_ffi.dylib"
  touch "$plugin_output_dir/.plugin-ready"
done

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

/usr/bin/python3 - "$ROOT/macos/Sources/AetowerBindings" "$ROOT/macos/Sources/aetower_ffiFFI" <<'PY'
import pathlib
import sys

for root in sys.argv[1:]:
    for path in pathlib.Path(root).rglob("*"):
        if not path.is_file():
            continue
        text = path.read_text()
        cleaned = "\n".join(line.rstrip(" \t") for line in text.split("\n"))
        if cleaned != text:
            path.write_text(cleaned)
PY
