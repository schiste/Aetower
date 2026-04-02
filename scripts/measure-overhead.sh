#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/rust"

cargo run --release -p aetower-bench -- "$@"
