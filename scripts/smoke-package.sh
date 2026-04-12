#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT/dist/Aetower.app"
APP_BIN="$APP_DIR/Contents/MacOS/Aetower"
FFI_LIB="$APP_DIR/Contents/Frameworks/libaetower_ffi.dylib"
HELPER_BIN="$APP_DIR/Contents/Helpers/aetower-helper"
MCP_PROXY_BIN="$APP_DIR/Contents/Helpers/aetower-mcp"
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
[ -f "$FFI_LIB" ]
[ -x "$HELPER_BIN" ]
[ -x "$MCP_PROXY_BIN" ]

codesign --verify --deep --strict "$APP_DIR"

if [ "$LAUNCH" -eq 1 ]; then
    "$APP_BIN" >/tmp/aetower-smoke.log 2>&1 &
    APP_PID=$!
    trap 'kill "$APP_PID" 2>/dev/null || true; wait "$APP_PID" 2>/dev/null || true' EXIT INT TERM
    sleep 3
    ps -p "$APP_PID" >/dev/null 2>&1
fi

printf '✓ package smoke passed\n'
