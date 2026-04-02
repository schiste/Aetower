#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENTITLEMENTS_DEFAULT="$ROOT/macos/Aetower.entitlements"

export AETOWER_ENTITLEMENTS_PATH="${AETOWER_ENTITLEMENTS_PATH:-$ENTITLEMENTS_DEFAULT}"
export AETOWER_NOTARIZE="${AETOWER_NOTARIZE:-1}"
export AETOWER_STAPLE="${AETOWER_STAPLE:-1}"
sh "$ROOT/scripts/release-preflight.sh"

sh "$ROOT/scripts/package-macos.sh"
printf '✓ release package ready at %s\n' "$ROOT/dist/Aetower.app"
