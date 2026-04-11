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

"$CARGO_BIN" run --locked --release -p aetower-bench -- "$@"
