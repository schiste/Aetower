#!/bin/sh
# Assemble the WEBSITE deployment directory (dist/cloudflare-site).
#
# This is website-only. The release payload (/releases/*, /homebrew/*,
# /third-party-notices.md) lives in the separate aetower-releases Pages
# project (see scripts/prepare-release-payload.sh) and is served on
# aetower.dev by the infra/release-router worker, so website deploys can
# never take the Sparkle appcast or installers down.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE_SOURCE="$ROOT/site"
SITE_OUTPUT="${AETOWER_CLOUDFLARE_SITE_DIR:-$ROOT/dist/cloudflare-site}"
BRAND_ICON="${AETOWER_SITE_ICON_SOURCE:-$ROOT/assets/brand/aetower-app-icon-source.png}"
BRAND_PREVIEW="${AETOWER_SITE_ICON_PREVIEW:-$ROOT/assets/brand/aetower-app-icon-preview.png}"
FALLBACK_ICON="$ROOT/tmp/app-icon/Aetower.iconset/icon_512x512@2x.png"

rm -rf "$SITE_OUTPUT"
mkdir -p "$SITE_OUTPUT/assets"
cp "$SITE_SOURCE/index.html" "$SITE_OUTPUT/index.html"
cp "$SITE_SOURCE/_headers" "$SITE_OUTPUT/_headers"
# Static site assets checked into site/assets (e.g. product screenshots).
# The brand icon is generated separately below and may overwrite its slot.
if [ -d "$SITE_SOURCE/assets" ]; then
  cp -R "$SITE_SOURCE/assets/." "$SITE_OUTPUT/assets/"
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

printf 'prepared Cloudflare Pages website: %s\n' "$SITE_OUTPUT"
printf 'deploy with: sh scripts/deploy-website.sh\n'
