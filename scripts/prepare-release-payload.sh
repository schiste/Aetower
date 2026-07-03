#!/bin/sh
# Assemble the release-payload deployment directory (dist/releases-payload).
#
# Order matters: first sync the live payload down (Pages deploys are full
# snapshots — without the sync, a deploy from a fresh machine would delete
# every historical archive and Sparkle delta), then overlay this release's
# artifacts from dist/.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="${AETOWER_CLOUDFLARE_RELEASES_DIR:-$ROOT/dist/releases-payload}"
APPCAST_DIR="${AETOWER_APPCAST_DIR:-$ROOT/dist/appcast}"
RELEASE_ARCHIVE="${AETOWER_RELEASE_ARCHIVE:-$ROOT/dist/Aetower.zip}"
THIRD_PARTY_NOTICES="${AETOWER_THIRD_PARTY_NOTICES_PATH:-$ROOT/dist/THIRD-PARTY-NOTICES.md}"
HOMEBREW_CASK="${AETOWER_HOMEBREW_CASK_PATH:-$ROOT/dist/homebrew/Casks/aetower.rb}"
SOURCE_ARCHIVE_DIR="${AETOWER_SOURCE_ARCHIVE_DIR:-$ROOT/dist/source}"
DMG_INSTALLER="${AETOWER_DMG_PATH:-$ROOT/dist/Aetower.dmg}"
INCLUDE_DMG="${AETOWER_INCLUDE_DMG_IN_SITE:-0}"
PKG_INSTALLER="${AETOWER_PKG_PATH:-$ROOT/dist/Aetower.pkg}"
INCLUDE_PKG="${AETOWER_INCLUDE_PKG_IN_SITE:-0}"
SKIP_SYNC="${AETOWER_SKIP_PAYLOAD_SYNC:-0}"

if [ ! -d "$APPCAST_DIR" ] || [ ! -f "$APPCAST_DIR/appcast.xml" ]; then
    echo "missing appcast artifacts; run sh scripts/release-candidate.sh first" >&2
    exit 1
fi
if [ ! -f "$RELEASE_ARCHIVE" ]; then
    echo "missing release archive: $RELEASE_ARCHIVE" >&2
    exit 1
fi
if [ ! -f "$THIRD_PARTY_NOTICES" ]; then
    echo "missing third-party notices: $THIRD_PARTY_NOTICES" >&2
    echo "run sh scripts/generate-third-party-notices.sh first" >&2
    exit 1
fi
if [ ! -f "$SOURCE_ARCHIVE_DIR/Aetower-source.tar.gz" ]; then
    echo "missing source archive: $SOURCE_ARCHIVE_DIR/Aetower-source.tar.gz" >&2
    echo "run sh scripts/generate-source-archive.sh first" >&2
    exit 1
fi

# 1. Sync the currently-published payload (self-healing snapshot base).
if [ "$SKIP_SYNC" = "1" ]; then
    echo "WARNING: skipping live payload sync; the deploy will only contain local artifacts" >&2
    rm -rf "$OUT"
    mkdir -p "$OUT/releases" "$OUT/homebrew/Casks"
    cp "$ROOT/infra/releases-pages/_headers" "$OUT/_headers"
else
    sh "$ROOT/scripts/mirror-live-release-payload.sh"
fi

# 2. Overlay this release's artifacts (newer local files win).
for APPCAST_FILE in "$APPCAST_DIR"/*; do
    [ -f "$APPCAST_FILE" ] || continue
    cp "$APPCAST_FILE" "$OUT/releases/"
done
cp "$RELEASE_ARCHIVE" "$OUT/releases/Aetower.zip"
cp "$THIRD_PARTY_NOTICES" "$OUT/third-party-notices.md"
for SOURCE_ARCHIVE in "$SOURCE_ARCHIVE_DIR"/*; do
    [ -f "$SOURCE_ARCHIVE" ] || continue
    cp "$SOURCE_ARCHIVE" "$OUT/releases/"
done
if [ -f "$HOMEBREW_CASK" ]; then
    cp "$HOMEBREW_CASK" "$OUT/homebrew/Casks/aetower.rb"
fi
if [ "$INCLUDE_DMG" = "1" ]; then
    if [ ! -f "$DMG_INSTALLER" ]; then
        echo "missing dmg installer: $DMG_INSTALLER" >&2
        echo "run sh scripts/package-macos-dmg.sh first or omit AETOWER_INCLUDE_DMG_IN_SITE=1" >&2
        exit 1
    fi
    DMG_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$ROOT/dist/Aetower.app/Contents/Info.plist")"
    DMG_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$ROOT/dist/Aetower.app/Contents/Info.plist")"
    cp "$DMG_INSTALLER" "$OUT/releases/Aetower.dmg"
    cp "$DMG_INSTALLER" "$OUT/releases/Aetower-$DMG_VERSION-$DMG_BUILD.dmg"
fi
if [ "$INCLUDE_PKG" = "1" ]; then
    if [ ! -f "$PKG_INSTALLER" ]; then
        echo "missing pkg installer: $PKG_INSTALLER" >&2
        echo "run sh scripts/package-macos-pkg.sh first or omit AETOWER_INCLUDE_PKG_IN_SITE=1" >&2
        exit 1
    fi
    PKG_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$ROOT/dist/Aetower.app/Contents/Info.plist")"
    PKG_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$ROOT/dist/Aetower.app/Contents/Info.plist")"
    cp "$PKG_INSTALLER" "$OUT/releases/Aetower.pkg"
    cp "$PKG_INSTALLER" "$OUT/releases/Aetower-$PKG_VERSION-$PKG_BUILD.pkg"
fi

printf 'prepared release payload: %s\n' "$OUT"
printf 'deploy with: sh scripts/deploy-release-payload.sh\n'
