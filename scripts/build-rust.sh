#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo build --manifest-path rust/Cargo.toml -p aetower-ffi --release
