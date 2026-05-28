#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE_SOURCE="$ROOT/site"
SITE_OUTPUT="${AETOWER_CLOUDFLARE_SITE_DIR:-$ROOT/dist/cloudflare-site}"
APPCAST_DIR="${AETOWER_APPCAST_DIR:-$ROOT/dist/appcast}"
BRAND_ICON="${AETOWER_SITE_ICON_SOURCE:-$ROOT/assets/brand/aetower-app-icon-source.png}"
BRAND_PREVIEW="${AETOWER_SITE_ICON_PREVIEW:-$ROOT/assets/brand/aetower-app-icon-preview.png}"
FALLBACK_ICON="$ROOT/tmp/app-icon/Aetower.iconset/icon_512x512@2x.png"

if [ ! -d "$APPCAST_DIR" ] || [ ! -f "$APPCAST_DIR/appcast.xml" ]; then
    echo "missing appcast artifacts; run sh scripts/release-candidate.sh first" >&2
    exit 1
fi

rm -rf "$SITE_OUTPUT"
mkdir -p "$SITE_OUTPUT/assets" "$SITE_OUTPUT/releases"
cp "$SITE_SOURCE/index.html" "$SITE_OUTPUT/index.html"
cp "$SITE_SOURCE/_headers" "$SITE_OUTPUT/_headers"
cp "$APPCAST_DIR"/* "$SITE_OUTPUT/releases/"

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
