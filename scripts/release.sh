#!/bin/sh
set -eu

# One-shot direct-download release: preflight -> package (sign + notarize +
# staple) -> generate/append the Sparkle appcast. Reads the same AETOWER_*
# environment as the individual scripts (see docs/direct-download-release.md).
#
# This script does NOT upload anything — hosting is environment-specific. It
# finishes by printing exactly which files to publish and where.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
ARCHIVES_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"

printf '\n=== 1/3 release preflight ===\n'
sh "$ROOT/scripts/release-preflight.sh"

printf '\n=== 2/3 package (build + sign + notarize) ===\n'
sh "$ROOT/scripts/package-macos.sh"

printf '\n=== 3/3 generate Sparkle appcast ===\n'
sh "$ROOT/scripts/generate-sparkle-appcast.sh"

printf '\n=== release artifacts ready ===\n'
printf '  app:     %s\n' "$DIST_DIR/Aetower.app"
printf '  archive: %s\n' "$DIST_DIR/Aetower.zip"
printf '  appcast: %s/appcast.xml\n' "$ARCHIVES_DIR"
printf '\nPublish step (manual): upload the contents of\n  %s\n' "$ARCHIVES_DIR"
printf 'so that:\n'
printf '  - appcast.xml is reachable at AETOWER_APPCAST_URL (%s)\n' "${AETOWER_APPCAST_URL:-<unset>}"
printf '  - each archive resolves under the download URL prefix\n'
printf '    (AETOWER_DOWNLOAD_URL_PREFIX, or the directory of AETOWER_APPCAST_URL).\n'
