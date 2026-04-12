#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${AETOWER_MCP_BIN:-$ROOT/rust/target/debug/aetower-mcp}"

cd "$ROOT"

if [ ! -x "$BIN" ]; then
    echo "Aetower MCP proxy binary is missing at $BIN." >&2
    echo "Run: sh scripts/build-rust.sh" >&2
    exit 1
fi

"$BIN" "$@"
