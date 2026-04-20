#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${AETOWER_MCP_BIN:-$ROOT/rust/target/debug/aetower-mcp}"
SOCKET_PATH="${AETOWER_MCP_SOCKET_PATH:-$HOME/.aetower/mcp.sock}"
REQUIRE_LIVE=0
CHECK_CLAUDE=0
CHECK_CODEX=0

usage() {
    cat <<'EOF'
usage: sh scripts/mcp-discovery-smoke.sh [--require-live] [--check-claude] [--check-codex]

Validates the Aetower local MCP proxy path, optional live socket handshake,
and optional client discovery registrations without starting a second Aetower
engine.

Environment:
  AETOWER_MCP_BIN          override proxy binary path
  AETOWER_MCP_SOCKET_PATH  override live app socket path
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --require-live)
            REQUIRE_LIVE=1
            ;;
        --check-claude)
            CHECK_CLAUDE=1
            ;;
        --check-codex)
            CHECK_CODEX=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            exit 2
            ;;
    esac
    shift
done

if [ ! -x "$BIN" ]; then
    echo "missing executable MCP proxy: $BIN" >&2
    echo "run: sh scripts/build-rust.sh" >&2
    exit 1
fi

echo "proxy: $BIN"
echo "socket: $SOCKET_PATH"

if [ -S "$SOCKET_PATH" ]; then
    if {
        printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{}}}'
        printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
    } | "$BIN" "$SOCKET_PATH" >/tmp/aetower-mcp-discovery-smoke.out 2>/tmp/aetower-mcp-discovery-smoke.err; then
        LIVE_HANDSHAKE_OK=1
    else
        LIVE_HANDSHAKE_OK=0
    fi
    if [ "$LIVE_HANDSHAKE_OK" -eq 0 ]; then
        if [ "$REQUIRE_LIVE" -eq 1 ]; then
            cat /tmp/aetower-mcp-discovery-smoke.err >&2
            exit 1
        fi
        echo "live handshake: skipped (socket connect failed; use --require-live to make this fatal)"
    elif ! grep -q '"serverInfo"' /tmp/aetower-mcp-discovery-smoke.out; then
        echo "live MCP initialize response did not include serverInfo" >&2
        exit 1
    elif ! grep -q '"aetower_current_snapshot"' /tmp/aetower-mcp-discovery-smoke.out; then
        echo "live MCP tools/list response did not include aetower_current_snapshot" >&2
        exit 1
    else
        echo "live handshake: ok"
    fi
else
    if [ "$REQUIRE_LIVE" -eq 1 ]; then
        echo "live socket not found: $SOCKET_PATH" >&2
        exit 1
    fi
    echo "live handshake: skipped (socket not present)"
fi

if [ "$CHECK_CLAUDE" -eq 1 ]; then
    if ! command -v claude >/dev/null 2>&1; then
        echo "claude CLI not found" >&2
        exit 1
    fi
    if ! claude mcp get aetower | grep -Fq "$BIN"; then
        echo "claude CLI does not report aetower using $BIN" >&2
        exit 1
    fi
    echo "claude discovery: ok"
else
    echo "claude discovery: not requested"
fi

if [ "$CHECK_CODEX" -eq 1 ]; then
    CODEX_CONFIG="$HOME/.codex/config.toml"
    if [ ! -f "$CODEX_CONFIG" ]; then
        echo "codex config not found: $CODEX_CONFIG" >&2
        exit 1
    fi
    if ! grep -q '^\[mcp_servers\.aetower\]' "$CODEX_CONFIG"; then
        echo "codex config has no [mcp_servers.aetower] block" >&2
        exit 1
    fi
    if ! grep -Fq "command = \"$BIN\"" "$CODEX_CONFIG"; then
        echo "codex aetower block does not use $BIN" >&2
        exit 1
    fi
    echo "codex discovery: ok"
else
    echo "codex discovery: not requested"
fi
