#!/bin/sh
# Shared limits for Rust and Swift builds. Source after setting ROOT.
CARGO_BIN="${CARGO_BIN:-$(command -v cargo || printf '%s' cargo)}"
if [ -n "${HOME:-}" ] && [ -x "$HOME/.cargo/bin/cargo" ] \
    && [ "$CARGO_BIN" = "$HOME/.chau7/cto_bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
fi

if [ -z "${AETOWER_BUILD_JOBS:-}" ]; then
    build_cpus="$(sysctl -n hw.ncpu 2>/dev/null || printf '2')"
    build_memory="$(sysctl -n hw.memsize 2>/dev/null || printf '17179869184')"
    # Leave CPU headroom and budget 8 GiB of host RAM per compiler job.
    # This limits concurrency; it is not a hard process memory limit.
    AETOWER_BUILD_JOBS=$((build_cpus / 2))
    build_memory_jobs=$((build_memory / 8589934592))
    [ "$AETOWER_BUILD_JOBS" -le "$build_memory_jobs" ] || AETOWER_BUILD_JOBS="$build_memory_jobs"
    [ "$AETOWER_BUILD_JOBS" -le 4 ] || AETOWER_BUILD_JOBS=4
    [ "$AETOWER_BUILD_JOBS" -ge 1 ] || AETOWER_BUILD_JOBS=1
fi
case "$AETOWER_BUILD_JOBS" in
    ''|*[!0-9]*) echo "AETOWER_BUILD_JOBS must be a positive integer" >&2; exit 2 ;;
esac
if ! [ "$AETOWER_BUILD_JOBS" -gt 0 ] 2>/dev/null; then
    echo "AETOWER_BUILD_JOBS must be a positive integer" >&2
    exit 2
fi
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$AETOWER_BUILD_JOBS}"
export CARGO_BUILD_JOBS AETOWER_BUILD_JOBS
