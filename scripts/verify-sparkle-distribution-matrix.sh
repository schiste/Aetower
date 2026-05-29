#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
APP_DIR="${AETOWER_APP_DIR:-$DIST_DIR/Aetower.app}"
ZIP_PATH="${AETOWER_ZIP_PATH:-$DIST_DIR/Aetower.zip}"
APPCAST_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"
APPCAST_PATH="${AETOWER_APPCAST_PATH:-$APPCAST_DIR/appcast.xml}"
HOMEBREW_CASK_PATH="${AETOWER_HOMEBREW_CASK_PATH:-$DIST_DIR/homebrew/Casks/aetower.rb}"
DMG_PATH="${AETOWER_DMG_PATH:-$DIST_DIR/Aetower.dmg}"
REQUIRE_DMG="${AETOWER_REQUIRE_DMG_MATRIX:-0}"
CHECK_DMG_IF_PRESENT="${AETOWER_CHECK_DMG_MATRIX:-0}"
PKG_PATH="${AETOWER_PKG_PATH:-$DIST_DIR/Aetower.pkg}"
REQUIRE_PKG="${AETOWER_REQUIRE_PKG_MATRIX:-0}"
CHECK_PKG_IF_PRESENT="${AETOWER_CHECK_PKG_MATRIX:-0}"
VERIFY_CODESIGN="${AETOWER_MATRIX_VERIFY_CODESIGN:-1}"

usage() {
    cat <<EOF
usage: $0 [--require-dmg] [--check-dmg-if-present] [--require-pkg] [--check-pkg-if-present] [--skip-codesign]

Verifies that every generated distribution form resolves to the same
Sparkle-enabled Aetower.app:
  - dist/Aetower.app baseline bundle
  - dist/Aetower.zip Sparkle update archive
  - dist/appcast/appcast.xml Sparkle feed
  - dist/homebrew/Casks/aetower.rb Homebrew acquisition channel
  - dist/Aetower.dmg optional drag-and-drop installer
  - dist/Aetower.pkg optional installer payload

The .dmg check is skipped by default unless --require-dmg or
--check-dmg-if-present is passed.
The .pkg check is skipped by default unless --require-pkg or
--check-pkg-if-present is passed.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --require-dmg)
            REQUIRE_DMG=1
            shift
            ;;
        --check-dmg-if-present)
            CHECK_DMG_IF_PRESENT=1
            shift
            ;;
        --require-pkg)
            REQUIRE_PKG=1
            shift
            ;;
        --check-pkg-if-present)
            CHECK_PKG_IF_PRESENT=1
            shift
            ;;
        --skip-codesign)
            VERIFY_CODESIGN=0
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

fail() {
    echo "sparkle distribution matrix failed: $*" >&2
    exit 1
}

plist_value() {
    PLIST_PATH="$1"
    KEY="$2"
    /usr/libexec/PlistBuddy -c "Print :$KEY" "$PLIST_PATH" 2>/dev/null || true
}

require_file() {
    if [ ! -f "$1" ]; then
        fail "missing file: $1"
    fi
}

require_dir() {
    if [ ! -d "$1" ]; then
        fail "missing directory: $1"
    fi
}

sha256_file() {
    shasum -a 256 "$1" | awk '{ print $1 }'
}

find_app() {
    /usr/bin/find "$1" -name Aetower.app -type d -prune -print | /usr/bin/head -n 1
}

metadata_for_app() {
    APP_PATH="$1"
    PLIST_PATH="$APP_PATH/Contents/Info.plist"
    require_file "$PLIST_PATH"

    BUNDLE_ID="$(plist_value "$PLIST_PATH" CFBundleIdentifier)"
    VERSION="$(plist_value "$PLIST_PATH" CFBundleShortVersionString)"
    BUILD_NUMBER="$(plist_value "$PLIST_PATH" CFBundleVersion)"
    FEED_URL="$(plist_value "$PLIST_PATH" SUFeedURL)"
    PUBLIC_KEY="$(plist_value "$PLIST_PATH" SUPublicEDKey)"

    if [ -z "$BUNDLE_ID" ] || [ -z "$VERSION" ] || [ -z "$BUILD_NUMBER" ]; then
        fail "$APP_PATH has incomplete bundle identity metadata"
    fi
    if [ -z "$FEED_URL" ] || [ -z "$PUBLIC_KEY" ]; then
        fail "$APP_PATH is not Sparkle-enabled; missing SUFeedURL or SUPublicEDKey"
    fi
    case "$FEED_URL" in
        https://*) ;;
        *) fail "$APP_PATH has non-HTTPS Sparkle feed URL: $FEED_URL" ;;
    esac
    require_dir "$APP_PATH/Contents/Frameworks/Sparkle.framework"
}

capture_baseline() {
    metadata_for_app "$APP_DIR"
    BASE_BUNDLE_ID="$BUNDLE_ID"
    BASE_VERSION="$VERSION"
    BASE_BUILD_NUMBER="$BUILD_NUMBER"
    BASE_FEED_URL="$FEED_URL"
    BASE_PUBLIC_KEY="$PUBLIC_KEY"
}

compare_app_to_baseline() {
    LABEL="$1"
    CANDIDATE_APP="$2"
    metadata_for_app "$CANDIDATE_APP"

    [ "$BUNDLE_ID" = "$BASE_BUNDLE_ID" ] || fail "$LABEL bundle id mismatch: $BUNDLE_ID != $BASE_BUNDLE_ID"
    [ "$VERSION" = "$BASE_VERSION" ] || fail "$LABEL version mismatch: $VERSION != $BASE_VERSION"
    [ "$BUILD_NUMBER" = "$BASE_BUILD_NUMBER" ] || fail "$LABEL build mismatch: $BUILD_NUMBER != $BASE_BUILD_NUMBER"
    [ "$FEED_URL" = "$BASE_FEED_URL" ] || fail "$LABEL Sparkle feed mismatch: $FEED_URL != $BASE_FEED_URL"
    [ "$PUBLIC_KEY" = "$BASE_PUBLIC_KEY" ] || fail "$LABEL Sparkle public key mismatch"

    if [ "$VERIFY_CODESIGN" = "1" ]; then
        codesign --verify --deep --strict "$CANDIDATE_APP" \
            || fail "$LABEL app failed codesign verification"
    fi
}

contains_literal() {
    FILE_PATH="$1"
    NEEDLE="$2"
    grep -F "$NEEDLE" "$FILE_PATH" >/dev/null 2>&1
}

verify_appcast() {
    require_file "$APPCAST_PATH"

    if command -v xmllint >/dev/null 2>&1; then
        xmllint --noout "$APPCAST_PATH" >/dev/null 2>&1 \
            || fail "appcast is not well-formed XML: $APPCAST_PATH"
    fi

    ARCHIVE_NAME="Aetower-$BASE_VERSION-$BASE_BUILD_NUMBER.zip"
    ARCHIVE_PATH="$APPCAST_DIR/$ARCHIVE_NAME"
    require_file "$ARCHIVE_PATH"

    ARCHIVE_LENGTH="$(wc -c < "$ARCHIVE_PATH" | tr -d '[:space:]')"
    APPCAST_COMPACT="$TMP_ROOT/appcast.compact.xml"
    tr -d '[:space:]' < "$APPCAST_PATH" > "$APPCAST_COMPACT"

    contains_literal "$APPCAST_COMPACT" "<sparkle:version>$BASE_BUILD_NUMBER</sparkle:version>" \
        || fail "appcast does not contain build $BASE_BUILD_NUMBER"
    contains_literal "$APPCAST_COMPACT" "<sparkle:shortVersionString>$BASE_VERSION</sparkle:shortVersionString>" \
        || fail "appcast does not contain version $BASE_VERSION"
    contains_literal "$APPCAST_PATH" "$ARCHIVE_NAME" \
        || fail "appcast does not reference $ARCHIVE_NAME"
    contains_literal "$APPCAST_PATH" "length=\"$ARCHIVE_LENGTH\"" \
        || fail "appcast length does not match $ARCHIVE_NAME"
    contains_literal "$APPCAST_PATH" "sparkle:edSignature=" \
        || fail "appcast enclosure is missing Sparkle EdDSA signature"
}

verify_homebrew_cask() {
    require_file "$HOMEBREW_CASK_PATH"

    ruby -c "$HOMEBREW_CASK_PATH" >/dev/null 2>&1 \
        || fail "Homebrew cask is not valid Ruby: $HOMEBREW_CASK_PATH"

    EXPECTED_VERSION="$BASE_VERSION,$BASE_BUILD_NUMBER"
    CASK_VERSION="$(
        sed -n 's/^[[:space:]]*version "\(.*\)".*$/\1/p' "$HOMEBREW_CASK_PATH" \
            | /usr/bin/head -n 1
    )"
    [ "$CASK_VERSION" = "$EXPECTED_VERSION" ] \
        || fail "Homebrew cask version mismatch: $CASK_VERSION != $EXPECTED_VERSION"

    ARCHIVE_PATH="$APPCAST_DIR/Aetower-$BASE_VERSION-$BASE_BUILD_NUMBER.zip"
    EXPECTED_SHA="$(sha256_file "$ARCHIVE_PATH")"
    CASK_SHA="$(
        sed -n 's/^[[:space:]]*sha256 "\(.*\)".*$/\1/p' "$HOMEBREW_CASK_PATH" \
            | /usr/bin/head -n 1
    )"
    [ "$CASK_SHA" = "$EXPECTED_SHA" ] \
        || fail "Homebrew cask sha mismatch: $CASK_SHA != $EXPECTED_SHA"

    contains_literal "$HOMEBREW_CASK_PATH" "auto_updates true" \
        || fail "Homebrew cask must declare auto_updates true because Sparkle owns in-app updates"
    contains_literal "$HOMEBREW_CASK_PATH" "app \"Aetower.app\"" \
        || fail "Homebrew cask does not install Aetower.app"
    contains_literal "$HOMEBREW_CASK_PATH" "$BASE_FEED_URL" \
        || fail "Homebrew cask livecheck does not reference the app Sparkle feed"
}

verify_zip_file() {
    LABEL="$1"
    CANDIDATE_ZIP="$2"
    require_file "$CANDIDATE_ZIP"

    ZIP_DIR="$TMP_ROOT/$LABEL-zip"
    mkdir -p "$ZIP_DIR"
    ditto -x -k "$CANDIDATE_ZIP" "$ZIP_DIR"
    ZIP_APP="$(find_app "$ZIP_DIR")"
    [ -n "$ZIP_APP" ] || fail "$LABEL zip does not contain Aetower.app"
    compare_app_to_baseline "$LABEL zip" "$ZIP_APP"
}

verify_zip() {
    ARCHIVE_PATH="$APPCAST_DIR/Aetower-$BASE_VERSION-$BASE_BUILD_NUMBER.zip"
    require_file "$ZIP_PATH"
    require_file "$ARCHIVE_PATH"

    ZIP_SHA="$(sha256_file "$ZIP_PATH")"
    ARCHIVE_SHA="$(sha256_file "$ARCHIVE_PATH")"
    [ "$ZIP_SHA" = "$ARCHIVE_SHA" ] \
        || fail "direct zip sha mismatch: $ZIP_SHA != immutable Sparkle archive $ARCHIVE_SHA"

    verify_zip_file "direct" "$ZIP_PATH"
    verify_zip_file "sparkle-archive" "$ARCHIVE_PATH"
}

verify_dmg() {
    if [ "$REQUIRE_DMG" != "1" ] && [ "$CHECK_DMG_IF_PRESENT" != "1" ]; then
        printf 'sparkle distribution matrix: dmg skipped (optional)\n'
        return
    fi

    if [ ! -f "$DMG_PATH" ]; then
        if [ "$REQUIRE_DMG" = "1" ]; then
            fail "missing required dmg: $DMG_PATH"
        fi
        printf 'sparkle distribution matrix: dmg skipped (not present)\n'
        return
    fi

    DMG_MOUNT="$TMP_ROOT/dmg-mount"
    mkdir -p "$DMG_MOUNT"
    hdiutil attach "$DMG_PATH" -readonly -nobrowse -noverify -mountpoint "$DMG_MOUNT" -quiet

    DMG_APP="$(find_app "$DMG_MOUNT")"
    [ -n "$DMG_APP" ] || fail "dmg does not contain Aetower.app"
    compare_app_to_baseline "dmg" "$DMG_APP"

    [ -L "$DMG_MOUNT/Applications" ] || fail "dmg is missing /Applications drag target symlink"
    [ -f "$DMG_MOUNT/.background/aetower-dmg-background.png" ] \
        || fail "dmg is missing branded drag-and-drop background"

    hdiutil detach "$DMG_MOUNT" -quiet
    DMG_MOUNT=""
}

verify_pkg() {
    if [ "$REQUIRE_PKG" != "1" ] && [ "$CHECK_PKG_IF_PRESENT" != "1" ]; then
        printf 'sparkle distribution matrix: pkg skipped (optional)\n'
        return
    fi

    if [ ! -f "$PKG_PATH" ]; then
        if [ "$REQUIRE_PKG" = "1" ]; then
            fail "missing required pkg: $PKG_PATH"
        fi
        printf 'sparkle distribution matrix: pkg skipped (not present)\n'
        return
    fi

    pkgutil --payload-files "$PKG_PATH" | grep -F "Aetower.app/Contents/Info.plist" >/dev/null 2>&1 \
        || fail "pkg payload does not contain Aetower.app"

    PKG_EXPANDED="$TMP_ROOT/pkg-expanded"
    PKG_PAYLOAD_DIR="$TMP_ROOT/pkg-payload"
    pkgutil --expand "$PKG_PATH" "$PKG_EXPANDED"
    mkdir -p "$PKG_PAYLOAD_DIR"

    PAYLOAD_PATH="$(/usr/bin/find "$PKG_EXPANDED" -name Payload -type f -print | /usr/bin/head -n 1)"
    [ -n "$PAYLOAD_PATH" ] || fail "pkg does not contain an extractable Payload"

    (cd "$PKG_PAYLOAD_DIR" && pax -rzf "$PAYLOAD_PATH")
    PKG_APP="$(find_app "$PKG_PAYLOAD_DIR")"
    [ -n "$PKG_APP" ] || fail "pkg payload extraction did not produce Aetower.app"
    compare_app_to_baseline "pkg" "$PKG_APP"
}

require_dir "$APP_DIR"
require_file "$ZIP_PATH"

TMP_ROOT="$(mktemp -d /tmp/aetower-sparkle-matrix.XXXXXX)"
DMG_MOUNT=""
cleanup() {
    if [ -n "$DMG_MOUNT" ] && [ -d "$DMG_MOUNT" ]; then
        hdiutil detach "$DMG_MOUNT" -quiet >/dev/null 2>&1 || true
    fi
    rm -rf "$TMP_ROOT"
}
trap cleanup EXIT INT TERM

capture_baseline
compare_app_to_baseline "baseline" "$APP_DIR"
verify_zip
verify_appcast
verify_homebrew_cask
verify_dmg
verify_pkg

printf '✓ Sparkle distribution matrix passed\n'
printf '  bundle id: %s\n' "$BASE_BUNDLE_ID"
printf '  version:   %s\n' "$BASE_VERSION"
printf '  build:     %s\n' "$BASE_BUILD_NUMBER"
printf '  feed:      %s\n' "$BASE_FEED_URL"
