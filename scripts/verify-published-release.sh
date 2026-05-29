#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"

if [ -f "$ENV_FILE" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
fi

APPCAST_URL="${AETOWER_APPCAST_URL:-}"
DOWNLOAD_PREFIX="${AETOWER_DOWNLOAD_URL_PREFIX:-}"
VERSION="${AETOWER_VERSION:-}"
BUILD_NUMBER="${AETOWER_BUILD_NUMBER:-}"
PUBLIC_BASE_URL="${AETOWER_PUBLIC_BASE_URL:-}"
CURL_BIN="${CURL_BIN:-}"
VERIFY_DMG="${AETOWER_VERIFY_DMG:-0}"
VERIFY_PKG="${AETOWER_VERIFY_PKG:-0}"

if [ -z "$CURL_BIN" ]; then
    if [ -x /usr/bin/curl ]; then
        CURL_BIN="/usr/bin/curl"
    else
        CURL_BIN="$(command -v curl || printf '%s' curl)"
    fi
fi

if [ -z "$APPCAST_URL" ]; then
    echo "AETOWER_APPCAST_URL is required" >&2
    exit 1
fi
if [ -z "$VERSION" ]; then
    echo "AETOWER_VERSION is required" >&2
    exit 1
fi
if [ -z "$BUILD_NUMBER" ]; then
    echo "AETOWER_BUILD_NUMBER is required" >&2
    exit 1
fi
if [ -z "$DOWNLOAD_PREFIX" ]; then
    DOWNLOAD_PREFIX="$(dirname "$APPCAST_URL")/"
fi
case "$DOWNLOAD_PREFIX" in
    */) ;;
    *) DOWNLOAD_PREFIX="$DOWNLOAD_PREFIX/" ;;
esac
if [ -z "$PUBLIC_BASE_URL" ]; then
    PUBLIC_BASE_URL="${APPCAST_URL%/releases/appcast.xml}"
fi

TMP_DIR="$(mktemp -d /tmp/aetower-published-release.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM
APPCAST_FILE="$TMP_DIR/appcast.xml"
IMMUTABLE_ARCHIVE="Aetower-$VERSION-$BUILD_NUMBER.zip"
IMMUTABLE_URL="$DOWNLOAD_PREFIX$IMMUTABLE_ARCHIVE"
DIRECT_ZIP_URL="$PUBLIC_BASE_URL/releases/Aetower.zip"
DIRECT_DMG_URL="$PUBLIC_BASE_URL/releases/Aetower.dmg"
DIRECT_PKG_URL="$PUBLIC_BASE_URL/releases/Aetower.pkg"
NOTICES_URL="$PUBLIC_BASE_URL/third-party-notices.md"
HOMEBREW_CASK_URL="$PUBLIC_BASE_URL/homebrew/Casks/aetower.rb"
SOURCE_ARCHIVE_URL="$PUBLIC_BASE_URL/releases/Aetower-$VERSION-$BUILD_NUMBER-source.tar.gz"
SOURCE_ARCHIVE_LATEST_URL="$PUBLIC_BASE_URL/releases/Aetower-source.tar.gz"
DMG_URL="$PUBLIC_BASE_URL/releases/Aetower-$VERSION-$BUILD_NUMBER.dmg"
PKG_URL="$PUBLIC_BASE_URL/releases/Aetower-$VERSION-$BUILD_NUMBER.pkg"

check_url() {
    label="$1"
    url="$2"

    if "$CURL_BIN" -fsSIL "$url" >/dev/null 2>&1; then
        printf '  %s: reachable\n' "$label"
        return
    fi
    if "$CURL_BIN" -fsSL --range 0-0 "$url" >/dev/null 2>&1; then
        printf '  %s: reachable\n' "$label"
        return
    fi

    printf '  %s: unreachable (%s)\n' "$label" "$url" >&2
    return 1
}

printf 'verify published release\n'
printf '  expected version: %s\n' "$VERSION"
printf '  expected build:   %s\n' "$BUILD_NUMBER"
printf '  appcast:          %s\n' "$APPCAST_URL"

"$CURL_BIN" -fsSL "$APPCAST_URL" -o "$APPCAST_FILE"

if ! grep -F "<sparkle:version>$BUILD_NUMBER</sparkle:version>" "$APPCAST_FILE" >/dev/null 2>&1; then
    printf 'published appcast does not contain expected Sparkle build %s\n' "$BUILD_NUMBER" >&2
    exit 1
fi
if ! grep -F "<sparkle:shortVersionString>$VERSION</sparkle:shortVersionString>" "$APPCAST_FILE" >/dev/null 2>&1; then
    printf 'published appcast does not contain expected version %s\n' "$VERSION" >&2
    exit 1
fi
if ! grep -F "$IMMUTABLE_URL" "$APPCAST_FILE" >/dev/null 2>&1; then
    printf 'published appcast does not point at expected archive %s\n' "$IMMUTABLE_URL" >&2
    exit 1
fi

check_url "immutable Sparkle archive" "$IMMUTABLE_URL"
check_url "Sparkle ZIP" "$DIRECT_ZIP_URL"
check_url "Homebrew cask" "$HOMEBREW_CASK_URL"
check_url "versioned source archive" "$SOURCE_ARCHIVE_URL"
check_url "latest source archive" "$SOURCE_ARCHIVE_LATEST_URL"
if [ "$VERIFY_DMG" = "1" ]; then
    check_url "versioned drag-and-drop dmg" "$DMG_URL"
    check_url "latest drag-and-drop dmg" "$DIRECT_DMG_URL"
fi
if [ "$VERIFY_PKG" = "1" ]; then
    check_url "versioned signed pkg installer" "$PKG_URL"
    check_url "latest signed pkg installer" "$DIRECT_PKG_URL"
fi
check_url "third-party notices" "$NOTICES_URL"

printf '✓ published release verified\n'
