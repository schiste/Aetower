#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_BIN="${CARGO_BIN:-$HOME/.cargo/bin/cargo}"

cd "$ROOT"

"$CARGO_BIN" run --locked --manifest-path "$ROOT/rust/Cargo.toml" -p aetower-bench --release -- --verify-telemetry-loopback
