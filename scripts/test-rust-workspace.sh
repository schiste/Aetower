#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/build-settings.sh"
cd "$ROOT/rust"

# Resolve the test feature graph once. Serial execution preserves the shared
# storage-index isolation required by MCP and the helper; the other crates'
# tests are short and need no separate compilation graph.
exec "$CARGO_BIN" test --locked --manifest-path "$ROOT/rust/Cargo.toml" \
    --workspace -- --test-threads=1
