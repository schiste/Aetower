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

DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
APPCAST_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"
SOURCE_DIR="${AETOWER_SOURCE_ARCHIVE_DIR:-$DIST_DIR/source}"
LOCAL_CASK_PATH="${AETOWER_HOMEBREW_CASK_PATH:-$DIST_DIR/homebrew/Casks/aetower.rb}"
LOCAL_NOTICES_PATH="${AETOWER_THIRD_PARTY_NOTICES_PATH:-$DIST_DIR/THIRD-PARTY-NOTICES.md}"
APPCAST_URL="${AETOWER_APPCAST_URL:-}"
DOWNLOAD_PREFIX="${AETOWER_DOWNLOAD_URL_PREFIX:-}"
VERSION="${AETOWER_VERSION:-}"
BUILD_NUMBER="${AETOWER_BUILD_NUMBER:-}"
PUBLIC_BASE_URL="${AETOWER_PUBLIC_BASE_URL:-}"
CURL_BIN="${CURL_BIN:-}"
VERIFY_DMG="${AETOWER_VERIFY_DMG:-0}"
VERIFY_PKG="${AETOWER_VERIFY_PKG:-0}"
MAX_ARTIFACT_BYTES="${AETOWER_MAX_RELEASE_ARTIFACT_BYTES:-1073741824}"

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
PUBLIC_BASE_URL="${PUBLIC_BASE_URL%/}"

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
IMMUTABLE_LOCAL_PATH="$APPCAST_DIR/$IMMUTABLE_ARCHIVE"
DIRECT_ZIP_LOCAL_PATH="$DIST_DIR/Aetower.zip"
VERSIONED_SOURCE_LOCAL_PATH="$SOURCE_DIR/Aetower-$VERSION-$BUILD_NUMBER-source.tar.gz"
LATEST_SOURCE_LOCAL_PATH="$SOURCE_DIR/Aetower-source.tar.gz"
VERSIONED_SOURCE_CHECKSUM_LOCAL_PATH="$VERSIONED_SOURCE_LOCAL_PATH.sha256"
LATEST_SOURCE_CHECKSUM_LOCAL_PATH="$LATEST_SOURCE_LOCAL_PATH.sha256"

file_size() {
    if SIZE="$(stat -f%z "$1" 2>/dev/null)"; then
        printf '%s\n' "$SIZE"
    else
        stat -c%s "$1"
    fi
}

download_artifact() {
    LABEL="$1"
    URL="$2"
    DESTINATION="$3"
    printf '  downloading %s\n' "$LABEL"
    "$CURL_BIN" -fsSL --retry 3 --retry-delay 1 \
        --max-filesize "$MAX_ARTIFACT_BYTES" "$URL" -o "$DESTINATION"
}

compare_remote_artifact() {
    LABEL="$1"
    URL="$2"
    LOCAL_PATH="$3"
    REMOTE_NAME="$4"

    if [ ! -f "$LOCAL_PATH" ]; then
        printf '  %s: missing local artifact %s\n' "$LABEL" "$LOCAL_PATH" >&2
        return 1
    fi

    REMOTE_PATH="$TMP_DIR/$REMOTE_NAME"
    download_artifact "$LABEL" "$URL" "$REMOTE_PATH"
    LOCAL_SIZE="$(file_size "$LOCAL_PATH")"
    REMOTE_SIZE="$(file_size "$REMOTE_PATH")"
    if [ "$LOCAL_SIZE" != "$REMOTE_SIZE" ]; then
        printf '  %s: size mismatch local=%s remote=%s (%s)\n' \
            "$LABEL" "$LOCAL_SIZE" "$REMOTE_SIZE" "$URL" >&2
        return 1
    fi

    LOCAL_SHA256="$(shasum -a 256 "$LOCAL_PATH" | awk '{ print $1 }')"
    REMOTE_SHA256="$(shasum -a 256 "$REMOTE_PATH" | awk '{ print $1 }')"
    if [ "$LOCAL_SHA256" != "$REMOTE_SHA256" ]; then
        printf '  %s: sha256 mismatch local=%s remote=%s (%s)\n' \
            "$LABEL" "$LOCAL_SHA256" "$REMOTE_SHA256" "$URL" >&2
        return 1
    fi
    printf '  %s: verified (%s bytes, %s)\n' "$LABEL" "$REMOTE_SIZE" "$REMOTE_SHA256"
}

printf 'verify published release\n'
printf '  expected version: %s\n' "$VERSION"
printf '  expected build:   %s\n' "$BUILD_NUMBER"
printf '  appcast:          %s\n' "$APPCAST_URL"

compare_remote_artifact "published appcast" "$APPCAST_URL" "$APPCAST_DIR/appcast.xml" "published-appcast.xml"
cp "$TMP_DIR/published-appcast.xml" "$APPCAST_FILE"

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

APPCAST_ENCLOSURE="$(
    tr '\n' ' ' <"$APPCAST_FILE" \
        | grep -oE '<enclosure[^>]*>' \
        | grep -F "url=\"$IMMUTABLE_URL\"" \
        | head -n 1 || true
)"
if [ -z "$APPCAST_ENCLOSURE" ]; then
    printf 'published appcast is missing the expected immutable enclosure %s\n' "$IMMUTABLE_URL" >&2
    exit 1
fi
APPCAST_LENGTH="$(printf '%s\n' "$APPCAST_ENCLOSURE" | sed -n 's/.*length="\([0-9][0-9]*\)".*/\1/p')"
APPCAST_SIGNATURE="$(printf '%s\n' "$APPCAST_ENCLOSURE" | sed -n 's/.*sparkle:edSignature="\([^"]*\)".*/\1/p')"
if [ -z "$APPCAST_LENGTH" ] || [ -z "$APPCAST_SIGNATURE" ]; then
    printf 'published appcast enclosure must include numeric length and Sparkle EdDSA signature\n' >&2
    exit 1
fi

if [ ! -f "$IMMUTABLE_LOCAL_PATH" ]; then
    printf 'missing local immutable archive: %s\n' "$IMMUTABLE_LOCAL_PATH" >&2
    exit 1
fi
IMMUTABLE_SIZE="$(file_size "$IMMUTABLE_LOCAL_PATH")"
if [ "$IMMUTABLE_SIZE" != "$APPCAST_LENGTH" ]; then
    printf 'published appcast length mismatch: appcast=%s local=%s (%s)\n' \
        "$APPCAST_LENGTH" "$IMMUTABLE_SIZE" "$IMMUTABLE_LOCAL_PATH" >&2
    exit 1
fi
printf '  appcast enclosure: %s bytes, signature present\n' "$APPCAST_LENGTH"

compare_remote_artifact "immutable Sparkle archive" "$IMMUTABLE_URL" "$IMMUTABLE_LOCAL_PATH" "immutable.zip"
compare_remote_artifact "latest Sparkle ZIP" "$DIRECT_ZIP_URL" "$DIRECT_ZIP_LOCAL_PATH" "latest.zip"
IMMUTABLE_SHA256="$(shasum -a 256 "$IMMUTABLE_LOCAL_PATH" | awk '{ print $1 }')"
DIRECT_ZIP_SHA256="$(shasum -a 256 "$DIRECT_ZIP_LOCAL_PATH" | awk '{ print $1 }')"
if [ "$IMMUTABLE_SHA256" != "$DIRECT_ZIP_SHA256" ]; then
    printf 'latest Sparkle ZIP does not match immutable archive: %s != %s\n' \
        "$DIRECT_ZIP_SHA256" "$IMMUTABLE_SHA256" >&2
    exit 1
fi

compare_remote_artifact "Homebrew cask" "$HOMEBREW_CASK_URL" "$LOCAL_CASK_PATH" "aetower.rb"
CASK_VERSION="$(sed -n 's/^[[:space:]]*version "\(.*\)".*$/\1/p' "$TMP_DIR/aetower.rb" | head -n 1)"
CASK_SHA256="$(sed -n 's/^[[:space:]]*sha256 "\(.*\)".*$/\1/p' "$TMP_DIR/aetower.rb" | head -n 1)"
if [ "$CASK_VERSION" != "$VERSION,$BUILD_NUMBER" ]; then
    printf 'published Homebrew cask version mismatch: %s != %s,%s\n' \
        "$CASK_VERSION" "$VERSION" "$BUILD_NUMBER" >&2
    exit 1
fi
if [ "$CASK_SHA256" != "$IMMUTABLE_SHA256" ]; then
    printf 'published Homebrew cask sha256 mismatch: %s != %s\n' \
        "$CASK_SHA256" "$IMMUTABLE_SHA256" >&2
    exit 1
fi

compare_remote_artifact "versioned source archive" "$SOURCE_ARCHIVE_URL" "$VERSIONED_SOURCE_LOCAL_PATH" "versioned-source.tar.gz"
compare_remote_artifact "latest source archive" "$SOURCE_ARCHIVE_LATEST_URL" "$LATEST_SOURCE_LOCAL_PATH" "latest-source.tar.gz"
compare_remote_artifact "versioned source checksum" "$SOURCE_ARCHIVE_URL.sha256" "$VERSIONED_SOURCE_CHECKSUM_LOCAL_PATH" "versioned-source.sha256"
compare_remote_artifact "latest source checksum" "$SOURCE_ARCHIVE_LATEST_URL.sha256" "$LATEST_SOURCE_CHECKSUM_LOCAL_PATH" "latest-source.sha256"

verify_checksum_manifest() {
    LABEL="$1"
    ARCHIVE_PATH="$2"
    CHECKSUM_PATH="$3"
    EXPECTED_SHA256="$(shasum -a 256 "$ARCHIVE_PATH" | awk '{ print $1 }')"
    DECLARED_SHA256="$(awk 'NF { print $1; exit }' "$CHECKSUM_PATH")"
    if [ "$EXPECTED_SHA256" != "$DECLARED_SHA256" ]; then
        printf '  %s: checksum manifest mismatch declared=%s expected=%s\n' \
            "$LABEL" "$DECLARED_SHA256" "$EXPECTED_SHA256" >&2
        return 1
    fi
    printf '  %s: checksum manifest verified\n' "$LABEL"
}

verify_checksum_manifest "versioned source" "$VERSIONED_SOURCE_LOCAL_PATH" "$VERSIONED_SOURCE_CHECKSUM_LOCAL_PATH"
verify_checksum_manifest "latest source" "$LATEST_SOURCE_LOCAL_PATH" "$LATEST_SOURCE_CHECKSUM_LOCAL_PATH"
compare_remote_artifact "third-party notices" "$NOTICES_URL" "$LOCAL_NOTICES_PATH" "third-party-notices.md"
if [ "$VERIFY_DMG" = "1" ]; then
    compare_remote_artifact "versioned drag-and-drop dmg" "$DMG_URL" "$DIST_DIR/Aetower.dmg" "versioned.dmg"
    compare_remote_artifact "latest drag-and-drop dmg" "$DIRECT_DMG_URL" "$DIST_DIR/Aetower.dmg" "latest.dmg"
fi
if [ "$VERIFY_PKG" = "1" ]; then
    compare_remote_artifact "versioned signed pkg installer" "$PKG_URL" "$DIST_DIR/Aetower.pkg" "versioned.pkg"
    compare_remote_artifact "latest signed pkg installer" "$DIRECT_PKG_URL" "$DIST_DIR/Aetower.pkg" "latest.pkg"
fi

printf '✓ published release verified\n'
