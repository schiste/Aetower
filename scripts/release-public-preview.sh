#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"

usage() {
    cat <<EOF
usage: $0 [--prepare-only] [--with-dmg] [--without-dmg] [--with-pkg] [--without-pkg] [--skip-matrix] [--deploy-cloudflare|--publish-cloudflare] [--skip-public-verify]

Build the public Developer Preview release set:
  1. signed/notarized macOS app + zip
  2. Sparkle appcast and immutable update archive
  3. Homebrew cask artifact
  4. signed/notarized drag-and-drop dmg installer
  5. signed/notarized pkg installer artifact
  6. corresponding source archive
  7. third-party dependency/license inventory
  8. Sparkle distribution matrix verification
  9. Cloudflare Pages static payload

By default this does not publish to Cloudflare. Use --publish-cloudflare only
when the generated release should become visible to Sparkle clients.

Public builds include both .dmg and .pkg artifacts by default. Use
--without-dmg or --without-pkg only for local testing and never for a public
download page.
EOF
}

PREPARE_ONLY=0
DEPLOY_CLOUDFLARE=0
WITH_DMG="${AETOWER_RELEASE_WITH_DMG:-1}"
WITH_PKG="${AETOWER_RELEASE_WITH_PKG:-1}"
RUN_MATRIX=1
VERIFY_PUBLIC=1
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
        --without-pkg)
            WITH_PKG=0
            shift
            ;;
        --with-dmg)
            WITH_DMG=1
            shift
            ;;
        --without-dmg)
            WITH_DMG=0
            shift
            ;;
        --skip-matrix)
            RUN_MATRIX=0
            shift
            ;;
        --deploy-cloudflare|--publish-cloudflare)
            DEPLOY_CLOUDFLARE=1
            shift
            ;;
        --skip-public-verify)
            VERIFY_PUBLIC=0
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

printf '\n=== generate corresponding source archive ===\n'
sh "$ROOT/scripts/generate-source-archive.sh"

if [ "$WITH_DMG" -eq 1 ]; then
    printf '\n=== generate signed/notarized dmg installer ===\n'
    sh "$ROOT/scripts/package-macos-dmg.sh"
else
    printf '\n=== skip dmg installer ===\n'
    printf 'Use --without-dmg only for local testing.\n'
fi

if [ "$WITH_PKG" -eq 1 ]; then
    printf '\n=== generate signed/notarized pkg installer ===\n'
    sh "$ROOT/scripts/package-macos-pkg.sh"
else
    printf '\n=== skip pkg installer ===\n'
    printf 'Use --without-pkg only for local testing.\n'
fi

printf '\n=== generate third-party notices ===\n'
sh "$ROOT/scripts/generate-third-party-notices.sh"

if [ "$RUN_MATRIX" -eq 1 ]; then
    printf '\n=== verify Sparkle distribution matrix ===\n'
    MATRIX_ARGS=""
    if [ "$WITH_DMG" -eq 1 ]; then
        MATRIX_ARGS="$MATRIX_ARGS --require-dmg"
    fi
    if [ "$WITH_PKG" -eq 1 ]; then
        MATRIX_ARGS="$MATRIX_ARGS --require-pkg"
    fi
    # shellcheck disable=SC2086
    sh "$ROOT/scripts/verify-sparkle-distribution-matrix.sh" $MATRIX_ARGS
fi

printf '\n=== prepare Cloudflare Pages payload ===\n'
AETOWER_INCLUDE_DMG_IN_SITE="$WITH_DMG" \
    AETOWER_INCLUDE_PKG_IN_SITE="$WITH_PKG" \
    sh "$ROOT/scripts/prepare-cloudflare-site.sh"

printf '\n=== public preview release set ready ===\n'
printf '  macOS app:       %s\n' "$ROOT/dist/Aetower.app"
printf '  sparkle zip:     %s\n' "$ROOT/dist/Aetower.zip"
if [ "$WITH_DMG" -eq 1 ]; then
    printf '  dmg installer:   %s\n' "$ROOT/dist/Aetower.dmg"
fi
if [ "$WITH_PKG" -eq 1 ]; then
    printf '  pkg installer:   %s\n' "$ROOT/dist/Aetower.pkg"
fi
printf '  appcast dir:     %s\n' "$ROOT/dist/appcast"
printf '  homebrew cask:   %s\n' "$ROOT/dist/homebrew/Casks/aetower.rb"
printf '  source archive:  %s\n' "$ROOT/dist/source/Aetower-${AETOWER_VERSION}-${AETOWER_BUILD_NUMBER}-source.tar.gz"
printf '  notices:         %s\n' "$ROOT/dist/THIRD-PARTY-NOTICES.md"
printf '  cloudflare site: %s\n' "$SITE_OUTPUT"

if [ "$DEPLOY_CLOUDFLARE" -eq 1 ]; then
    printf '\n=== publish Cloudflare Pages ===\n'
    printf 'Publishing updates %s and makes this build discoverable by Sparkle clients.\n' "${AETOWER_APPCAST_URL:-the configured appcast URL}"
    npx wrangler pages deploy "$SITE_OUTPUT" --project-name "$CLOUDFLARE_PROJECT"
    if [ "$VERIFY_PUBLIC" -eq 1 ]; then
        printf '\n=== verify published release ===\n'
        AETOWER_VERIFY_DMG="$WITH_DMG" \
            AETOWER_VERIFY_PKG="$WITH_PKG" \
            sh "$ROOT/scripts/verify-published-release.sh"
    else
        printf '\nPublished release verification skipped by --skip-public-verify.\n'
    fi
else
    printf '\nCloudflare publish intentionally skipped.\n'
    printf 'Publish explicitly with:\n'
    printf '  sh scripts/release-public-preview.sh --prepare-only --publish-cloudflare\n'
fi
