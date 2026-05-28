#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"

usage() {
    cat <<EOF
usage: $0 [--prepare-only] [--with-pkg] [--deploy-cloudflare]

Build the public Developer Preview release set:
  1. signed/notarized macOS app + zip
  2. Sparkle appcast and immutable update archive
  3. Homebrew cask artifact
  4. Cloudflare Pages static payload

By default this does not deploy to Cloudflare. Use --deploy-cloudflare only
after reviewing the generated dist/cloudflare-site payload.

By default this also skips the .pkg installer because public .pkg output
requires a Developer ID Installer certificate. Use --with-pkg when that
certificate is installed.
EOF
}

PREPARE_ONLY=0
DEPLOY_CLOUDFLARE=0
WITH_PKG=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prepare-only)
            PREPARE_ONLY=1
            shift
            ;;
        --with-pkg)
            WITH_PKG=1
            shift
            ;;
        --deploy-cloudflare)
            DEPLOY_CLOUDFLARE=1
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
    echo "release env file not found: $ENV_FILE" >&2
    echo "copy .env.release.example to .env.release.local and fill private values." >&2
    exit 1
fi

CLOUDFLARE_PROJECT="${AETOWER_CLOUDFLARE_PROJECT:-aetower-dev}"
SITE_OUTPUT="${AETOWER_CLOUDFLARE_SITE_DIR:-$ROOT/dist/cloudflare-site}"

if [ "$PREPARE_ONLY" -eq 0 ]; then
    printf '\n=== build signed/notarized macOS release ===\n'
    sh "$ROOT/scripts/release-candidate.sh"
else
    printf '\n=== prepare-only: reuse existing dist release artifacts ===\n'
fi

printf '\n=== generate Homebrew cask ===\n'
sh "$ROOT/scripts/generate-homebrew-cask.sh"

if [ "$WITH_PKG" -eq 1 ]; then
    printf '\n=== generate signed/notarized pkg installer ===\n'
    sh "$ROOT/scripts/package-macos-pkg.sh"
else
    printf '\n=== skip pkg installer ===\n'
    printf 'Use --with-pkg after installing a Developer ID Installer certificate.\n'
fi

printf '\n=== prepare Cloudflare Pages payload ===\n'
sh "$ROOT/scripts/prepare-cloudflare-site.sh"

printf '\n=== public preview release set ready ===\n'
printf '  macOS app:       %s\n' "$ROOT/dist/Aetower.app"
printf '  direct zip:      %s\n' "$ROOT/dist/Aetower.zip"
if [ "$WITH_PKG" -eq 1 ]; then
    printf '  pkg installer:   %s\n' "$ROOT/dist/Aetower.pkg"
fi
printf '  appcast dir:     %s\n' "$ROOT/dist/appcast"
printf '  homebrew cask:   %s\n' "$ROOT/dist/homebrew/Casks/aetower.rb"
printf '  cloudflare site: %s\n' "$SITE_OUTPUT"

if [ "$DEPLOY_CLOUDFLARE" -eq 1 ]; then
    printf '\n=== deploy Cloudflare Pages ===\n'
    npx wrangler pages deploy "$SITE_OUTPUT" --project-name "$CLOUDFLARE_PROJECT"
else
    printf '\nCloudflare deploy intentionally skipped.\n'
    printf 'Deploy explicitly with:\n'
    printf '  npx wrangler pages deploy %s --project-name %s\n' "$SITE_OUTPUT" "$CLOUDFLARE_PROJECT"
fi
