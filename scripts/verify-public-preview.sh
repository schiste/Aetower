#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"
APP_DIR="$ROOT/dist/Aetower.app"
APP_BIN="$APP_DIR/Contents/MacOS/Aetower"

RUN_PREFLIGHT=0
RUN_PACKAGE=0
RUN_MATRIX=0
RUN_GATEKEEPER=0
RUN_OPERATOR=0
RUN_SOAK=0
REBUILD=0
REQUIRE_PKG=0
SOAK_SECONDS="${AETOWER_PUBLIC_PREVIEW_SOAK_SECONDS:-7200}"

usage() {
    cat <<EOF
usage: $0 [--all] [--preflight] [--package] [--matrix] [--require-pkg] [--gatekeeper] [--operator] [--soak] [--rebuild]

Validate a public Developer Preview candidate.

Defaults are explicit: pass --all for the normal public-preview battery, or
select individual checks. The soak defaults to 2 hours; override with
AETOWER_PUBLIC_PREVIEW_SOAK_SECONDS.
EOF
}

if [ "$#" -eq 0 ]; then
    usage
    exit 1
fi

while [ "$#" -gt 0 ]; do
    case "$1" in
        --all)
            RUN_PREFLIGHT=1
            RUN_PACKAGE=1
            RUN_MATRIX=1
            RUN_GATEKEEPER=1
            RUN_OPERATOR=1
            RUN_SOAK=1
            shift
            ;;
        --preflight)
            RUN_PREFLIGHT=1
            shift
            ;;
        --package)
            RUN_PACKAGE=1
            shift
            ;;
        --matrix)
            RUN_MATRIX=1
            shift
            ;;
        --require-pkg)
            REQUIRE_PKG=1
            shift
            ;;
        --gatekeeper)
            RUN_GATEKEEPER=1
            shift
            ;;
        --operator)
            RUN_OPERATOR=1
            shift
            ;;
        --soak)
            RUN_SOAK=1
            shift
            ;;
        --rebuild)
            REBUILD=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unsupported argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [ -f "$ENV_FILE" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
fi

if [ "$RUN_PREFLIGHT" -eq 1 ]; then
    sh "$ROOT/scripts/release-candidate.sh" --preflight-only
fi

if [ "$RUN_PACKAGE" -eq 1 ]; then
    if [ "$REBUILD" -eq 1 ]; then
        sh "$ROOT/scripts/smoke-package.sh" --rebuild
    else
        sh "$ROOT/scripts/smoke-package.sh"
    fi
fi

if [ "$RUN_MATRIX" -eq 1 ]; then
    if [ "$REQUIRE_PKG" -eq 1 ]; then
        sh "$ROOT/scripts/verify-sparkle-distribution-matrix.sh" --require-pkg
    else
        sh "$ROOT/scripts/verify-sparkle-distribution-matrix.sh"
    fi
fi

if [ "$RUN_GATEKEEPER" -eq 1 ]; then
    if [ ! -x "$APP_BIN" ]; then
        echo "missing packaged app at $APP_DIR; run --package --rebuild first" >&2
        exit 1
    fi
    spctl --assess --type execute --verbose=4 "$APP_DIR"
fi

if [ "$RUN_OPERATOR" -eq 1 ]; then
    if [ "$REBUILD" -eq 1 ]; then
        sh "$ROOT/scripts/local-operator-smoke.sh" --rebuild
    else
        sh "$ROOT/scripts/local-operator-smoke.sh"
    fi
fi

if [ "$RUN_SOAK" -eq 1 ]; then
    if [ "$SOAK_SECONDS" -lt 7200 ]; then
        echo "public-preview soak must be at least 7200 seconds" >&2
        echo "set AETOWER_PUBLIC_PREVIEW_SOAK_SECONDS=7200 or higher" >&2
        exit 1
    fi
    AETOWER_SOAK_DURATION_SECONDS="$SOAK_SECONDS" \
        AETOWER_SOAK_INTERVAL_SECONDS="${AETOWER_SOAK_INTERVAL_SECONDS:-10}" \
        AETOWER_SOAK_SAMPLE_SECONDS="${AETOWER_SOAK_SAMPLE_SECONDS:-10}" \
        sh "$ROOT/scripts/soak-local.sh" --launch
fi

printf '✓ public-preview validation checks passed\n'
