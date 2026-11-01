#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
. "$ROOT/scripts/build-settings.sh"
cd "$ROOT/rust"

# Keep the generator's UniFFI CLI/serde features out of the runtime test graph.
# The bridge build can then reuse the runtime dependencies tested here. Keep
# every crate's tests, including the generator, serial for storage isolation.
"$CARGO_BIN" test --locked --manifest-path "$ROOT/rust/Cargo.toml" \
    --workspace --exclude uniffi-bindgen-swift -- --test-threads=1
# Keep the waiting shell: exec can discard earlier child CPU accounting on macOS.
"$CARGO_BIN" test --locked --manifest-path "$ROOT/rust/Cargo.toml" \
    -p uniffi-bindgen-swift -- --test-threads=1
exit "$?"
