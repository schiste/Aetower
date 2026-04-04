#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DURATION_SECONDS="${AETOWER_SOAK_DURATION_SECONDS:-1800}"
INTERVAL_SECONDS="${AETOWER_SOAK_INTERVAL_SECONDS:-10}"
SAMPLE_SECONDS="${AETOWER_SOAK_SAMPLE_SECONDS:-10}"
SETTLE_SECONDS="${AETOWER_PROFILE_SETTLE_SECONDS:-20}"
REBUILD=0
LAUNCH=0
CHECK_RELEASE=0

while [ "$#" -gt 0 ]; do
    case "$1" in
        --rebuild)
            REBUILD=1
            shift
            ;;
        --launch)
            LAUNCH=1
            shift
            ;;
        --release-preflight)
            CHECK_RELEASE=1
            shift
            ;;
        *)
            echo "unsupported argument: $1" >&2
            exit 1
            ;;
    esac
done

if [ "$CHECK_RELEASE" -eq 1 ]; then
    sh "$ROOT/scripts/release-preflight.sh"
fi

sh "$ROOT/scripts/telemetry-smoke.sh"

ARGS="--duration $DURATION_SECONDS --interval $INTERVAL_SECONDS --sample-seconds $SAMPLE_SECONDS --enforce"
if [ "$REBUILD" -eq 1 ]; then
    ARGS="$ARGS --rebuild"
fi
if [ "$LAUNCH" -eq 1 ]; then
    ARGS="$ARGS --launch"
fi

AETOWER_PROFILE_SETTLE_SECONDS="$SETTLE_SECONDS" sh "$ROOT/scripts/profile-runtime.sh" $ARGS
