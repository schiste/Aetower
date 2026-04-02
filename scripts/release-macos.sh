#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENTITLEMENTS_DEFAULT="$ROOT/macos/Aetower.entitlements"

if [ -z "${AETOWER_SIGN_IDENTITY:-}" ]; then
    echo "AETOWER_SIGN_IDENTITY is required for a release build." >&2
    echo "Available signing identities:" >&2
    security find-identity -v -p codesigning || true
    exit 1
fi

export AETOWER_ENTITLEMENTS_PATH="${AETOWER_ENTITLEMENTS_PATH:-$ENTITLEMENTS_DEFAULT}"
export AETOWER_NOTARIZE="${AETOWER_NOTARIZE:-1}"
export AETOWER_STAPLE="${AETOWER_STAPLE:-1}"

if [ "${AETOWER_NOTARIZE}" = "1" ] && [ -z "${AETOWER_NOTARY_PROFILE:-}" ]; then
    echo "AETOWER_NOTARY_PROFILE is required when notarization is enabled." >&2
    exit 1
fi

sh "$ROOT/scripts/package-macos.sh"
printf '✓ release package ready at %s\n' "$ROOT/dist/Aetower.app"
