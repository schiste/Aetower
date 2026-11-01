#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/build-settings.sh"
cd "$ROOT/rust"

"$CARGO_BIN" run --locked --release -p aetower-bench -- "$@"
