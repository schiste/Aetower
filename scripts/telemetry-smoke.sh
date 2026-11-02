#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/build-settings.sh"

cd "$ROOT"

"$CARGO_BIN" run --locked --manifest-path "$ROOT/rust/Cargo.toml" -p aetower-bench --release -- --verify-telemetry-loopback
