#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE_SOURCE="$ROOT/site"
SITE_OUTPUT="${AETOWER_CLOUDFLARE_SITE_DIR:-$ROOT/dist/cloudflare-site}"
APPCAST_DIR="${AETOWER_APPCAST_DIR:-$ROOT/dist/appcast}"
RELEASE_ARCHIVE="${AETOWER_RELEASE_ARCHIVE:-$ROOT/dist/Aetower.zip}"
BRAND_ICON="${AETOWER_SITE_ICON_SOURCE:-$ROOT/assets/brand/aetower-app-icon-source.png}"
BRAND_PREVIEW="${AETOWER_SITE_ICON_PREVIEW:-$ROOT/assets/brand/aetower-app-icon-preview.png}"
THIRD_PARTY_NOTICES="${AETOWER_THIRD_PARTY_NOTICES_PATH:-$ROOT/dist/THIRD-PARTY-NOTICES.md}"
HOMEBREW_CASK="${AETOWER_HOMEBREW_CASK_PATH:-$ROOT/dist/homebrew/Casks/aetower.rb}"
PKG_INSTALLER="${AETOWER_PKG_PATH:-$ROOT/dist/Aetower.pkg}"
INCLUDE_PKG="${AETOWER_INCLUDE_PKG_IN_SITE:-0}"
FALLBACK_ICON="$ROOT/tmp/app-icon/Aetower.iconset/icon_512x512@2x.png"

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

rm -rf "$SITE_OUTPUT"
mkdir -p "$SITE_OUTPUT/assets" "$SITE_OUTPUT/homebrew/Casks" "$SITE_OUTPUT/releases"
cp "$SITE_SOURCE/index.html" "$SITE_OUTPUT/index.html"
cp "$SITE_SOURCE/_headers" "$SITE_OUTPUT/_headers"
cp "$APPCAST_DIR"/* "$SITE_OUTPUT/releases/"
cp "$RELEASE_ARCHIVE" "$SITE_OUTPUT/releases/Aetower.zip"
cp "$THIRD_PARTY_NOTICES" "$SITE_OUTPUT/third-party-notices.md"
if [ -f "$HOMEBREW_CASK" ]; then
    cp "$HOMEBREW_CASK" "$SITE_OUTPUT/homebrew/Casks/aetower.rb"
fi
if [ "$INCLUDE_PKG" = "1" ]; then
    if [ ! -f "$PKG_INSTALLER" ]; then
        echo "missing pkg installer: $PKG_INSTALLER" >&2
        echo "run sh scripts/package-macos-pkg.sh first or omit AETOWER_INCLUDE_PKG_IN_SITE=1" >&2
        exit 1
    fi
    PKG_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$ROOT/dist/Aetower.app/Contents/Info.plist")"
    PKG_BUILD="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$ROOT/dist/Aetower.app/Contents/Info.plist")"
    cp "$PKG_INSTALLER" "$SITE_OUTPUT/releases/Aetower.pkg"
    cp "$PKG_INSTALLER" "$SITE_OUTPUT/releases/Aetower-$PKG_VERSION-$PKG_BUILD.pkg"
fi

if [ -f "$BRAND_PREVIEW" ]; then
    cp "$BRAND_PREVIEW" "$SITE_OUTPUT/assets/aetower-app-icon-preview.png"
elif [ -f "$BRAND_ICON" ]; then
    sips -z 1024 1024 "$BRAND_ICON" --out "$SITE_OUTPUT/assets/aetower-app-icon-preview.png" >/dev/null
elif [ -f "$FALLBACK_ICON" ]; then
    cp "$FALLBACK_ICON" "$SITE_OUTPUT/assets/aetower-app-icon-preview.png"
else
    sh "$ROOT/scripts/generate-app-icon.sh" >/dev/null
    cp "$FALLBACK_ICON" "$SITE_OUTPUT/assets/aetower-app-icon-preview.png"
fi

printf 'prepared Cloudflare Pages site: %s\n' "$SITE_OUTPUT"
printf 'deploy with: npx wrangler pages deploy %s --project-name aetower-dev\n' "$SITE_OUTPUT"
