#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODE="pre-commit"
BENCH_ITERATIONS=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --mode)
            MODE="${2:-}"
            shift 2
            ;;
        --iterations)
            BENCH_ITERATIONS="${2:-}"
            shift 2
            ;;
        *)
            echo "unsupported argument: $1" >&2
            exit 1
            ;;
    esac
done

if [ -z "$BENCH_ITERATIONS" ]; then
    case "$MODE" in
        pre-commit) BENCH_ITERATIONS="8" ;;
        pre-push|full) BENCH_ITERATIONS="20" ;;
        *)
            echo "unsupported mode: $MODE" >&2
            exit 1
            ;;
    esac
fi

run() {
    printf '\n==> %s\n' "$1"
    shift
    "$@"
}

cd "$ROOT"

run "cargo fmt --check" cargo fmt --manifest-path "$ROOT/rust/Cargo.toml" --all -- --check
run "cargo clippy" cargo clippy --locked --manifest-path "$ROOT/rust/Cargo.toml" --all-targets -- -D warnings
run "cargo test" cargo test --locked --manifest-path "$ROOT/rust/Cargo.toml"
run "build Rust bridge" sh "$ROOT/scripts/build-rust.sh"
run "swift build" swift build --package-path "$ROOT/macos"
run "benchmark budget" sh "$ROOT/scripts/measure-overhead.sh" --iterations "$BENCH_ITERATIONS" --enforce

if [ "$MODE" = "pre-push" ] || [ "$MODE" = "full" ]; then
    run "telemetry smoke" sh "$ROOT/scripts/telemetry-smoke.sh"
    run "package smoke" sh "$ROOT/scripts/smoke-package.sh" --rebuild
fi

printf '\n✓ local ci passed (%s)\n' "$MODE"
