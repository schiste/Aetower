#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT/dist/Aetower.app"
APP_BIN="$APP_DIR/Contents/MacOS/Aetower"
APP_PLIST="$APP_DIR/Contents/Info.plist"
APP_ICON="$APP_DIR/Contents/Resources/Aetower.icns"
FFI_LIB="$APP_DIR/Contents/Frameworks/libaetower_ffi.dylib"
HELPER_BIN="$APP_DIR/Contents/Helpers/aetower-helper"
MCP_PROXY_BIN="$APP_DIR/Contents/Helpers/aetower-mcp"
CLI_BIN="$APP_DIR/Contents/Helpers/aetower"
REBUILD=0
LAUNCH=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --rebuild)
            REBUILD=1
            shift
            ;;
        --launch)
            LAUNCH=1
            shift
            ;;
        *)
            echo "unsupported argument: $1" >&2
            exit 1
            ;;
    esac
done

if [ "$REBUILD" -eq 1 ] || [ ! -d "$APP_DIR" ]; then
    sh "$ROOT/scripts/package-macos.sh"
fi

[ -x "$APP_BIN" ]
[ -f "$APP_PLIST" ]
[ -f "$APP_ICON" ]
[ -f "$FFI_LIB" ]
[ -x "$MCP_PROXY_BIN" ]
[ -x "$CLI_BIN" ]
# The operator CLI must at least answer --version without a running app; the
# socket-backed verbs are exercised by the live smoke below.
"$CLI_BIN" --version >/dev/null
/usr/libexec/PlistBuddy -c "Print :CFBundleIconFile" "$APP_PLIST" | grep -Fx "Aetower" >/dev/null
# The privileged Endpoint Security helper is optional and excluded by default
# (package-macos.sh only bundles it when AETOWER_INCLUDE_PRIVILEGED_HELPER=1),
# so only assert its presence when it was meant to be included.
if [ "${AETOWER_INCLUDE_PRIVILEGED_HELPER:-0}" = "1" ]; then
    [ -x "$HELPER_BIN" ]
fi

codesign --verify --deep --strict "$APP_DIR"

if [ "$LAUNCH" -eq 1 ]; then
    "$APP_BIN" >/tmp/aetower-smoke.log 2>&1 &
    APP_PID=$!
    trap 'kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true' EXIT INT TERM
    sleep 3
    ps -p "$APP_PID" >/dev/null 2>&1
fi

printf '✓ package smoke passed\n'
