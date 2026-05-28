#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"

usage() {
    cat <<EOF
usage: $0 [--preflight-only]

Build a signed, notarized Developer Preview release candidate.

The script loads .env.release.local when present, applies conservative public
preview defaults, runs release preflight, then runs scripts/release.sh unless
--preflight-only is passed.
EOF
}

PREFLIGHT_ONLY=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --preflight-only)
            PREFLIGHT_ONLY=1
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
else
    printf 'release env file not found: %s\n' "$ENV_FILE" >&2
    printf 'copy .env.release.example to .env.release.local and fill private values.\n' >&2
fi

if [ -z "${AETOWER_SIGN_IDENTITY:-}" ]; then
    detected_identity="$(
        (
            security find-identity -v -p codesigning 2>/dev/null || true
            if command -v zsh >/dev/null 2>&1; then
                zsh -lc 'security find-identity -v -p codesigning' 2>/dev/null || true
            fi
        ) \
            | awk -F'"' '/Developer ID Application:/ { print $2; exit }' \
            | head -n 1
    )"
    if [ -n "$detected_identity" ]; then
        export AETOWER_SIGN_IDENTITY="$detected_identity"
        printf 'using detected Developer ID identity: %s\n' "$AETOWER_SIGN_IDENTITY"
    fi
fi

export AETOWER_NOTARIZE="${AETOWER_NOTARIZE:-1}"
export AETOWER_STAPLE="${AETOWER_STAPLE:-1}"
export AETOWER_NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-aetower-notary}"
export AETOWER_INCLUDE_PRIVILEGED_HELPER="${AETOWER_INCLUDE_PRIVILEGED_HELPER:-0}"
export AETOWER_REQUIRE_ENDPOINT_SECURITY="${AETOWER_REQUIRE_ENDPOINT_SECURITY:-0}"
export AETOWER_REQUIRE_SPARKLE="${AETOWER_REQUIRE_SPARKLE:-1}"

sh "$ROOT/scripts/release-preflight.sh"

if [ "$PREFLIGHT_ONLY" -eq 1 ]; then
    exit 0
fi

sh "$ROOT/scripts/release.sh"
