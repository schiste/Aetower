#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/build-settings.sh"
PROFILE=debug
while [ "$#" -gt 0 ]; do
    case "$1" in
        --profile)
            PROFILE="${2:-}"
            [ "$#" -ge 2 ] || { echo "--profile requires debug, release or all" >&2; exit 2; }
            shift 2
            ;;
        --help|-h)
            echo "usage: $0 [--profile debug|release|all] (default: debug)"
            exit 0
            ;;
        *) echo "unsupported argument: $1" >&2; exit 2 ;;
    esac
done
case "$PROFILE" in
    debug|release) PROFILES="$PROFILE" ;;
    all) PROFILES="debug release" ;;
    *) echo "--profile must be debug, release or all" >&2; exit 2 ;;
esac

cd "$ROOT/rust"
for profile in $PROFILES; do
    if [ "$profile" = release ]; then
        set -- --release
    else
        set --
    fi
    # Resolve the runtime feature graph once per profile. Keep bindgen separate:
    # its uniffi CLI features must not become dependencies of the shipped engine.
    "$CARGO_BIN" build --locked -p aetower-mcp -p aetower-cli -p aetower-ffi "$@"
    dylib="$ROOT/rust/target/$profile/libaetower_ffi.dylib"
    if [ "$(otool -D "$dylib" | tail -n 1)" != '@rpath/libaetower_ffi.dylib' ]; then
        install_name_tool -id '@rpath/libaetower_ffi.dylib' "$dylib"
    fi
    # The app links the dylib. Remove archives left by older restored caches,
    # so a successful app build cannot leave a stale static bridge behind.
    rm -f "$ROOT/rust/target/$profile/libaetower_ffi.a" \
        "$ROOT/rust/target/$profile/deps/libaetower_ffi.a"
done

# Build the host generator in debug even when generating release bindings.
"$CARGO_BIN" build --locked -p uniffi-bindgen-swift
UNIFFI_BINDGEN="$ROOT/rust/target/debug/uniffi-bindgen-swift"
if [ "$PROFILE" = release ]; then
    BINDINGS_DYLIB="$ROOT/rust/target/release/libaetower_ffi.dylib"
else
    BINDINGS_DYLIB="$ROOT/rust/target/debug/libaetower_ffi.dylib"
fi

# Validate content before skipping generation; stale or damaged outputs regenerate.
# Keep the shell alive to account for every waited compiler/generator child.
status=0
/usr/bin/python3 "$ROOT/scripts/prepare-swift-bindings.py" --generate \
    "$UNIFFI_BINDGEN" "$BINDINGS_DYLIB" "$ROOT/macos/Sources" "$ROOT/tmp" || status=$?
exit "$status"
