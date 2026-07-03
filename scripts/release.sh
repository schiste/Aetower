#!/bin/sh
set -eu

# One-shot direct-download release: preflight -> package (sign + notarize +
# staple) -> generate/append the Sparkle appcast -> corresponding source
# archive -> third-party notices. Reads the same AETOWER_* environment as the
# individual scripts (see
# docs/direct-download-release.md).
#
# This script does NOT upload anything — hosting is environment-specific. It
# finishes by printing exactly which files to publish and where.

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
ARCHIVES_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"

export AETOWER_NOTARIZE="${AETOWER_NOTARIZE:-1}"
export AETOWER_STAPLE="${AETOWER_STAPLE:-1}"

# The flagship .pkg needs a Developer ID *Installer* certificate (distinct from
# the Developer ID *Application* cert used to sign the app). When it's absent we
# skip the pkg rather than fail the whole release — the zip and dmg still ship.
has_installer_identity() {
    [ -n "${AETOWER_INSTALLER_SIGN_IDENTITY:-}" ] && return 0
    [ "${AETOWER_ALLOW_UNSIGNED_PKG:-0}" = "1" ] && return 0
    security find-identity -v -p basic 2>/dev/null | grep -q "Developer ID Installer:"
}

printf '\n=== 1/8 release preflight ===\n'
sh "$ROOT/scripts/release-preflight.sh"

printf '\n=== 2/8 package (build + sign + notarize) ===\n'
sh "$ROOT/scripts/package-macos.sh"

printf '\n=== 3/8 generate Sparkle appcast ===\n'
sh "$ROOT/scripts/generate-sparkle-appcast.sh"

printf '\n=== 4/8 signed installer package (.pkg) ===\n'
if has_installer_identity; then
    sh "$ROOT/scripts/package-macos-pkg.sh"
else
    printf '  skipped: no Developer ID Installer identity available.\n'
    printf '  (zip + dmg still ship; set AETOWER_INSTALLER_SIGN_IDENTITY to build the pkg.)\n'
fi

printf '\n=== 5/8 disk image (.dmg) ===\n'
sh "$ROOT/scripts/package-macos-dmg.sh"

printf '\n=== 6/8 Homebrew cask ===\n'
sh "$ROOT/scripts/generate-homebrew-cask.sh"

printf '\n=== 7/8 generate corresponding source archive ===\n'
sh "$ROOT/scripts/generate-source-archive.sh"

printf '\n=== 8/8 generate third-party notices ===\n'
sh "$ROOT/scripts/generate-third-party-notices.sh"

printf '\n=== release artifacts ready ===\n'
printf '  app:     %s\n' "$DIST_DIR/Aetower.app"
printf '  archive: %s\n' "$DIST_DIR/Aetower.zip"
[ -f "$DIST_DIR/Aetower.pkg" ] && printf '  pkg:     %s\n' "$DIST_DIR/Aetower.pkg"
[ -f "$DIST_DIR/Aetower.dmg" ] && printf '  dmg:     %s\n' "$DIST_DIR/Aetower.dmg"
printf '  appcast: %s/appcast.xml\n' "$ARCHIVES_DIR"
printf '  source:  %s/source/Aetower-source.tar.gz\n' "$DIST_DIR"
printf '  notices: %s/THIRD-PARTY-NOTICES.md\n' "$DIST_DIR"
printf '\nPublish step (manual): upload the contents of\n  %s\n' "$ARCHIVES_DIR"
printf 'so that:\n'
printf '  - appcast.xml is reachable at AETOWER_APPCAST_URL (%s)\n' "${AETOWER_APPCAST_URL:-<unset>}"
printf '  - each archive resolves under the download URL prefix\n'
printf '    (AETOWER_DOWNLOAD_URL_PREFIX, or the directory of AETOWER_APPCAST_URL).\n'
