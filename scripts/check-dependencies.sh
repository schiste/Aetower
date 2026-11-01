#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/build-settings.sh"
deny_bin="$(command -v cargo-deny || true)"
if [ -z "$deny_bin" ]; then
    echo "missing required tool: cargo-deny (install with: cargo install cargo-deny --locked)" >&2
    exit 1
fi
cd "$ROOT/rust"
CARGO="$CARGO_BIN" "$deny_bin" check advisories bans sources \
    --disable-fetch --config "$ROOT/deny.toml"
